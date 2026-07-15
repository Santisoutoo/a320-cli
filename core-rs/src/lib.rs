//! Core del simulador headless del A320.
//!
//! Envuelve los crates vendorizados de FlyByWire (`systems` + `a320_systems`)
//! en un runtime persistente e interactivo, con una capa de API limpia
//! (`set`/`get`/`step`/`run`/`set_environment`/`snapshot`) que sirve por igual
//! a la CLI y al servidor MCP.
//!
//! Módulos:
//! - [`variables`]: registro de variables + reader/writer persistentes.
//! - [`environment`]: borde con el "mundo" (tabla de simvars de `UpdateContext`).
//! - [`runtime`]: runtime persistente (tick loop) sobre `Simulation<A320>`.
//! - [`controls`]: catálogo curado de controles accionables (`list_controls`).
//! - [`api`]: capa de control/observación limpia (una API, dos frontends).

pub mod api;
pub mod controls;
pub mod environment;
pub mod runtime;
pub mod variables;
