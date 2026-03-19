pub mod proto;
pub mod resolver;

pub use proto::{
    ParsedProto, ProtoElement, ProtoParser, ErrorSeverity,
    MessageElement,
};
pub use resolver::ImportResolver;
