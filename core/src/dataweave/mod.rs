//! DataWeave conversion module.
//!
//! Extracts DataWeave expressions from the Mule AST and converts them to
//! Java beans using an LLM provider. Falls back to structured TODO stubs
//! when no LLM is configured.

pub mod converter;
pub mod patterns;
