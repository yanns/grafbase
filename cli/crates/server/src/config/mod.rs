use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::Ordering,
    time::{Duration, SystemTime},
};

use common::{
    consts::{GENERATED_SCHEMAS_DIR, GRAFBASE_SCHEMA_FILE_NAME},
    environment::{Environment, Project, SchemaLocation},
};
use common_types::UdfKind;
use engine::Registry;
use futures_util::stream::BoxStream;
use tokio::process::Command;

use crate::{
    atomics::REGISTRY_PARSED_EPOCH_OFFSET_MILLIS,
    consts::{
        CONFIG_PARSER_SCRIPT_CJS, CONFIG_PARSER_SCRIPT_ESM, ENTRYPOINT_SCRIPT_FILE_NAME, SCHEMA_PARSER_DIR,
        TS_NODE_SCRIPT_PATH,
    },
    node::validate_node,
};

mod actor;
mod error;
mod parser;

pub use self::{actor::ConfigActor, error::ConfigError};

#[derive(Debug, Clone)]
pub struct DetectedUdf {
    pub udf_name: String,
    pub udf_kind: UdfKind,
    pub fresh: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub(crate) registry: Registry,
    pub(crate) detected_udfs: Vec<DetectedUdf>,
    pub(crate) federated_graph_config: Option<parser_sdl::federation::FederatedGraphConfig>,

    // The file that triggered this change (if any)
    pub(crate) triggering_file: Option<PathBuf>,
}

pub type ConfigStream = BoxStream<'static, Config>;

/// Builds the configuration for the current project.
///
/// Either by building & running grafbase.config.ts or parsing grafbase.schema
pub(crate) async fn build_config(
    environment_variables: &HashMap<String, String>,
    triggering_file: Option<PathBuf>,
) -> Result<Config, ConfigError> {
    trace!("parsing schema");
    let project = Project::get();

    let schema_path = match project.schema_path.location() {
        SchemaLocation::TsConfig(ref ts_config_path) => {
            let written_schema_path = parse_and_generate_config_from_ts(ts_config_path).await?;

            Cow::Owned(written_schema_path)
        }
        SchemaLocation::Graphql(ref path) => Cow::Borrowed(path.to_str().ok_or(ConfigError::ProjectPath)?),
    };
    let schema = tokio::fs::read_to_string(Path::new(schema_path.as_ref())).await?;

    let parser::ParserResult {
        registry,
        required_udfs,
        federated_graph_config,
    } = parser::parse_sdl(&schema, environment_variables).await?;

    let offset = REGISTRY_PARSED_EPOCH_OFFSET_MILLIS.load(Ordering::Acquire);
    let registry_mtime = SystemTime::UNIX_EPOCH.checked_add(Duration::from_millis(offset));
    let detected_resolvers = futures_util::future::join_all(required_udfs.into_iter().map(|(udf_kind, udf_name)| {
        // Last file to be written to in the build process.
        let entrypoint_path = project
            .udfs_build_artifact_path(udf_kind)
            .join(&udf_name)
            .join(ENTRYPOINT_SCRIPT_FILE_NAME);
        async move {
            let entrypoint_mtime = tokio::fs::metadata(&entrypoint_path)
                .await
                .ok()
                .map(|metadata| metadata.modified().expect("must be supported"));
            let fresh = registry_mtime
                .zip(entrypoint_mtime)
                .map(|(registry_mtime, entrypoint_mtime)| entrypoint_mtime > registry_mtime)
                .unwrap_or_default();
            DetectedUdf {
                udf_name,
                udf_kind,
                fresh,
            }
        }
    }))
    .await;

    REGISTRY_PARSED_EPOCH_OFFSET_MILLIS.store(
        u64::try_from(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap(),
        Ordering::Release,
    );

    Ok(Config {
        registry,
        detected_udfs: detected_resolvers,
        federated_graph_config,
        triggering_file,
    })
}

/// Parses a TypeScript Grafbase configuration and generates a GraphQL schema
/// file to the filesystem, returning a path to the generated file.
async fn parse_and_generate_config_from_ts(ts_config_path: &Path) -> Result<String, ConfigError> {
    let environment = Environment::get();
    let project = Project::get();

    let generated_schemas_dir = project.dot_grafbase_directory_path.join(GENERATED_SCHEMAS_DIR);
    let generated_config_path = generated_schemas_dir.join(GRAFBASE_SCHEMA_FILE_NAME);

    if !generated_schemas_dir.exists() {
        std::fs::create_dir_all(generated_schemas_dir)?;
    }

    let module_type = project
        .package_json_path
        .as_deref()
        .and_then(ModuleType::from_package_json)
        .unwrap_or_default();

    let config_parser_path = environment
        .user_dot_grafbase_path
        .join(SCHEMA_PARSER_DIR)
        .join(match module_type {
            ModuleType::CommonJS => CONFIG_PARSER_SCRIPT_CJS,
            ModuleType::Esm => CONFIG_PARSER_SCRIPT_ESM,
        });

    let ts_node_path = environment.user_dot_grafbase_path.join(TS_NODE_SCRIPT_PATH);

    let args = match module_type {
        ModuleType::CommonJS => vec![
            ts_node_path.to_string_lossy().to_string(),
            config_parser_path.to_string_lossy().to_string(),
            ts_config_path.to_string_lossy().to_string(),
            generated_config_path.to_string_lossy().to_string(),
        ],
        ModuleType::Esm => vec![
            ts_node_path.to_string_lossy().to_string(),
            "--compilerOptions".to_string(),
            r#"{"module": "esnext", "moduleResolution": "node", "esModuleInterop": true}"#.to_string(),
            "--esm".to_string(),
            config_parser_path.to_string_lossy().to_string(),
            ts_config_path.to_string_lossy().to_string(),
            generated_config_path.to_string_lossy().to_string(),
        ],
    };

    validate_node().await?;
    let node_command = Command::new("node")
        .args(args)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let output = node_command.wait_with_output().await?;

    if !output.status.success() {
        let msg = String::from_utf8_lossy(&output.stderr);
        return Err(ConfigError::LoadTsConfig(msg.into_owned()));
    }

    let generated_config_path = generated_config_path.to_str().ok_or(ConfigError::ProjectPath)?;

    trace!("Generated configuration in {}.", generated_config_path);

    Ok(generated_config_path.to_string())
}

#[derive(Default)]
enum ModuleType {
    #[default]
    CommonJS,
    Esm,
}

impl ModuleType {
    pub fn from_package_json(package_json: &Path) -> Option<ModuleType> {
        let value = serde_json::from_slice::<serde_json::Value>(&std::fs::read(package_json).ok()?).ok()?;
        if value["type"].as_str()? == "module" {
            Some(ModuleType::Esm)
        } else {
            Some(ModuleType::CommonJS)
        }
    }
}
