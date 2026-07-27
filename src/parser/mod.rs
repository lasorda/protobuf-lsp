pub mod proto;
pub mod resolver;

pub use proto::{
    ParsedProto, ProtoElement, ProtoParser, ErrorSeverity, ParseError,
    MessageElement,
};
pub use resolver::ImportResolver;
