pub mod completion;
pub mod definition;
pub mod hover;
pub mod symbols;
pub mod formatting;
pub mod diagnostics;

pub use completion::provide_completion;
pub use definition::provide_definition_async;
pub use hover::provide_hover;
pub use symbols::provide_document_symbols;
pub use formatting::format_document;
pub use diagnostics::{validate_proto_file, create_parse_diagnostics};
