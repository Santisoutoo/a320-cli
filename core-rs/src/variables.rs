//! Registro de variables persistente para el runtime headless.
//!
//! Todo lo que el avión de FBW lee y escribe pasa por dos traits públicos
//! (`systems/src/simulation/mod.rs`):
//!
//! - [`VariableRegistry`]: asigna un [`VariableIdentifier`] a cada nombre de
//!   variable la primera vez que se pide (`get`).
//! - [`SimulatorReaderWriter`]: lee/escribe `f64` por identificador — es el
//!   almacén real de valores de todas las variables.
//!
//! El test bed de FBW implementa estos traits con tipos **privados**
//! (`TestVariableRegistry` / `TestReaderWriter`, `test.rs:618-686`), lo que
//! impide enumerar o volcar el registro desde fuera. Aquí replicamos ese
//! patrón con tipos **públicos y persistentes** entre ticks, y conservamos el
//! índice nombre→id para que la API (`set`/`get`/`list_variables`/`snapshot`)
//! pueda resolver variables por nombre.
//!
//! Detalle clave (write-on-demand): `get()` acuña un identificador para
//! cualquier nombre que se le pida. Una variable que el avión aún no ha tocado
//! simplemente no está en el almacén, así que su lectura devuelve un valor por
//! defecto documentado ([`UNWRITTEN_DEFAULT`], igual que el test double de FBW).

use std::collections::{BTreeMap, HashMap};

use systems::simulation::{SimulatorReaderWriter, VariableIdentifier, VariableRegistry};

/// Valor devuelto al leer una variable que nunca se ha escrito.
///
/// Reproduce el comportamiento del `TestReaderWriter` de FBW, que devuelve
/// `0.0` para cualquier identificador desconocido.
pub const UNWRITTEN_DEFAULT: f64 = 0.0;

/// Implementación pública y persistente de [`VariableRegistry`].
///
/// Mantiene el mapa nombre→`VariableIdentifier`. A diferencia del
/// `TestVariableRegistry` de FBW, este vive durante toda la sesión del runtime
/// y su índice es inspeccionable (`iter`, `names`, `find`), lo que da
/// `list_variables()` y el acceso por nombre "gratis".
#[derive(Debug, Default)]
pub struct PersistentVariableRegistry {
    name_to_identifier: HashMap<String, VariableIdentifier>,
    next_identifier: VariableIdentifier,
}

impl PersistentVariableRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Devuelve el identificador ya asignado a `name`, sin acuñar uno nuevo.
    pub fn find(&self, name: &str) -> Option<VariableIdentifier> {
        self.name_to_identifier.get(name).copied()
    }

    /// Itera los nombres registrados (orden no determinista).
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.name_to_identifier.keys()
    }

    /// Itera los pares (nombre, identificador) registrados.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &VariableIdentifier)> {
        self.name_to_identifier.iter()
    }

    /// Número de variables registradas.
    pub fn len(&self) -> usize {
        self.name_to_identifier.len()
    }

    pub fn is_empty(&self) -> bool {
        self.name_to_identifier.is_empty()
    }
}

impl VariableRegistry for PersistentVariableRegistry {
    fn get(&mut self, name: String) -> VariableIdentifier {
        match self.name_to_identifier.get(&name).copied() {
            Some(identifier) => identifier,
            None => {
                let identifier = self.next_identifier;
                self.name_to_identifier.insert(name, identifier);
                self.next_identifier = identifier.next();

                identifier
            }
        }
    }

    fn get_unprefixed(&mut self, name: String) -> VariableIdentifier {
        self.get(name)
    }
}

/// Implementación pública y persistente de [`SimulatorReaderWriter`].
///
/// Es el almacén (backing store) de todos los valores `f64` indexados por
/// identificador. Persiste entre ticks: lo que el avión escribe en un tick lo
/// lee en el siguiente, y nuestras escrituras (controles / entorno) también
/// sobreviven. Un volcado de este mapa es la base de `snapshot()`.
#[derive(Debug, Default)]
pub struct PersistentReaderWriter {
    values: HashMap<VariableIdentifier, f64>,
}

impl PersistentReaderWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// ¿Se ha escrito alguna vez este identificador?
    pub fn contains(&self, identifier: &VariableIdentifier) -> bool {
        self.values.contains_key(identifier)
    }

    /// Acceso de solo lectura al mapa completo id→valor.
    pub fn values(&self) -> &HashMap<VariableIdentifier, f64> {
        &self.values
    }
}

impl SimulatorReaderWriter for PersistentReaderWriter {
    fn read(&mut self, identifier: &VariableIdentifier) -> f64 {
        *self.values.get(identifier).unwrap_or(&UNWRITTEN_DEFAULT)
    }

    fn write(&mut self, identifier: &VariableIdentifier, value: f64) {
        self.values.insert(*identifier, value);
    }
}

/// Une el registro y el almacén en un solo objeto y ofrece las utilidades de
/// alto nivel que la API necesita: lectura/escritura por nombre,
/// `list_variables()` y `snapshot()`.
///
/// El runtime (issue #7) pasa `&mut store.registry` a `Simulation::new` y
/// `&mut store.reader_writer` a `Simulation::tick` (préstamos disjuntos de
/// campos, permitidos por Rust). Como el registro se comparte con el avión en
/// la construcción, los identificadores que el avión cachea coinciden con los
/// de este índice: escribir una var de entorno por nombre acaba bajo el mismo
/// id que el avión leerá en el tick.
#[derive(Debug, Default)]
pub struct VariableStore {
    pub registry: PersistentVariableRegistry,
    pub reader_writer: PersistentReaderWriter,
}

impl VariableStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resuelve `name` a su identificador, acuñando uno en el primer uso.
    pub fn id_of(&mut self, name: &str) -> VariableIdentifier {
        self.registry.get(name.to_owned())
    }

    /// Escribe `value` en la variable `name` (acuña id si es nueva).
    pub fn write_by_name(&mut self, name: &str, value: f64) {
        let identifier = self.registry.get(name.to_owned());
        self.reader_writer.write(&identifier, value);
    }

    /// Lee la variable `name` (acuña id si es nueva; devuelve el default si
    /// nunca se escribió).
    pub fn read_by_name(&mut self, name: &str) -> f64 {
        let identifier = self.registry.get(name.to_owned());
        self.reader_writer.read(&identifier)
    }

    /// Lee sin acuñar id para nombres desconocidos. Un nombre no registrado
    /// devuelve [`UNWRITTEN_DEFAULT`] sin modificar el registro.
    pub fn peek_by_name(&self, name: &str) -> f64 {
        match self.registry.find(name) {
            Some(identifier) => self
                .reader_writer
                .values()
                .get(&identifier)
                .copied()
                .unwrap_or(UNWRITTEN_DEFAULT),
            None => UNWRITTEN_DEFAULT,
        }
    }

    /// Nombres de todas las variables registradas, ordenados.
    pub fn list_variables(&self) -> Vec<String> {
        let mut names: Vec<String> = self.registry.names().cloned().collect();
        names.sort();
        names
    }

    /// Volcado completo nombre→valor de todas las variables conocidas.
    /// `BTreeMap` para que el orden sea estable/determinista.
    pub fn snapshot(&self) -> BTreeMap<String, f64> {
        self.registry
            .iter()
            .map(|(name, identifier)| {
                let value = self
                    .reader_writer
                    .values()
                    .get(identifier)
                    .copied()
                    .unwrap_or(UNWRITTEN_DEFAULT);
                (name.clone(), value)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registering_a_name_is_idempotent() {
        let mut registry = PersistentVariableRegistry::new();

        let first = registry.get("ELEC_AC_1_BUS_IS_POWERED".to_owned());
        let again = registry.get("ELEC_AC_1_BUS_IS_POWERED".to_owned());

        assert_eq!(first, again, "same name must map to the same identifier");
        assert_eq!(registry.len(), 1, "no duplicate registration");
    }

    #[test]
    fn distinct_names_get_distinct_identifiers() {
        let mut registry = PersistentVariableRegistry::new();

        let a = registry.get("A".to_owned());
        let b = registry.get("B".to_owned());

        assert_ne!(a, b);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn a_write_is_readable_back() {
        let mut store = VariableStore::new();

        store.write_by_name("SIM ON GROUND", 1.0);

        assert_eq!(store.read_by_name("SIM ON GROUND"), 1.0);
    }

    #[test]
    fn unwritten_variable_returns_default() {
        let mut store = VariableStore::new();

        // Nunca escrita: debe devolver el default documentado (0.0).
        assert_eq!(store.read_by_name("NEVER WRITTEN"), UNWRITTEN_DEFAULT);
        assert_eq!(UNWRITTEN_DEFAULT, 0.0);
    }

    #[test]
    fn peek_does_not_mint_identifiers() {
        let store = VariableStore::new();

        assert_eq!(store.peek_by_name("UNKNOWN"), UNWRITTEN_DEFAULT);
        assert!(store.registry.is_empty(), "peek must not register names");
    }

    #[test]
    fn list_variables_reports_registered_names_sorted() {
        let mut store = VariableStore::new();
        store.write_by_name("ZULU", 1.0);
        store.write_by_name("ALPHA", 2.0);

        assert_eq!(
            store.list_variables(),
            vec!["ALPHA".to_owned(), "ZULU".to_owned()]
        );
    }

    #[test]
    fn snapshot_dumps_every_known_variable() {
        let mut store = VariableStore::new();
        store.write_by_name("A", 1.0);
        store.write_by_name("B", 2.0);
        // Registrada pero nunca escrita (acuñada por lectura): sale con default.
        let _ = store.read_by_name("C");

        let snap = store.snapshot();
        assert_eq!(snap.get("A"), Some(&1.0));
        assert_eq!(snap.get("B"), Some(&2.0));
        assert_eq!(snap.get("C"), Some(&UNWRITTEN_DEFAULT));
    }
}
