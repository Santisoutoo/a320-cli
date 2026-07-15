//! Core del simulador headless del A320.
//!
//! Envuelve los crates vendorizados de FlyByWire (`systems` + `a320_systems`)
//! en un runtime persistente e interactivo, con una capa de API limpia
//! (`set`/`get`/`step`/`run`/`set_environment`/`snapshot`) que sirve por igual
//! a la CLI y al servidor MCP.
//!
//! Módulos:
//! - [`variables`]: registro de variables + reader/writer persistentes.

pub mod variables;
