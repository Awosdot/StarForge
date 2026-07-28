pub mod interface;
pub mod loader;
pub mod manifest;
pub mod registry;
pub mod ai;

pub use interface::{Plugin, PluginDeclaration, PluginRegistrar};
pub use loader::{PluginLoadError, PluginManager};
pub use ai::{AIPlugin, AIPluginDeclaration, AIPluginRegistrar, AIRequest, AIResponse, AICapability};
