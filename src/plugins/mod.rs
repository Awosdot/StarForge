pub mod ai;
pub mod interface;
pub mod loader;
pub mod manifest;
pub mod registry;

pub use ai::{
    AICapability, AIPlugin, AIPluginDeclaration, AIPluginRegistrar, AIRequest, AIResponse,
};
pub use interface::{Plugin, PluginDeclaration, PluginRegistrar};
pub use loader::{PluginLoadError, PluginManager};
