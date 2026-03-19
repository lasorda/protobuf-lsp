pub mod proto;
pub mod resolver;

pub use proto::{
    ParsedProto, ProtoElement, ProtoParser, ErrorSeverity,
    MessageElement, EnumElement, EnumValueElement, ServiceElement,
    MethodElement, FieldElement, ImportElement,
};
pub use resolver::ImportResolver;
