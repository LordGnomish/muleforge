//! Abstract syntax trees used internally.
//!
//! Two distinct ASTs live here:
//! - `mule_ast`: faithful representation of the input Mule 4 application.
//! - `camel_ir`: intermediate representation of the output Camel Quarkus project.
//!
//! The mapper transforms `MuleAst → CamelIr`.

pub mod camel_ir;
pub mod mule_ast;
