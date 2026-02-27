use shared_resources::messages::protocol::CentralSyncMessage;
use std::collections::HashMap;
use std::io::{self, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex, RwLock};

/// Información de un servidor regional registrado
#[derive(Debug, Clone)]
pub struct RegionalInfo {
    pub region_id: u8,
    pub sync_address: String,
}

/// Registro de servidores regionales
pub struct RegionalRegistry {
    // Mapeo nombre -> info
    regions: Arc<RwLock<HashMap<String, RegionalInfo>>>,
    // Mapeo ID -> nombre (para lookup rápido)
    id_to_name: Arc<RwLock<HashMap<u8, String>>>,
    // Contador para asignar IDs únicos
    next_id: Arc<Mutex<u8>>,
}

impl RegionalRegistry {
    pub fn new() -> Self {
        Self {
            regions: Arc::new(RwLock::new(HashMap::new())),
            id_to_name: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)), // IDs empiezan en 1
        }
    }

    /// Registra un servidor regional (o recupera su info si ya existe)
    /// Retorna (region_id, was_new)
    pub fn register(&self, name: String, sync_address: String) -> io::Result<(u8, bool)> {
        // Validar nombre alfanumérico
        if !is_valid_region_name(&name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Nombre de región inválido: '{}'. Debe ser alfanumérico",
                    name
                ),
            ));
        }

        // Verificar si ya existe
        {
            let regions = self.regions.read().unwrap();
            if let Some(info) = regions.get(&name) {
                println!(
                    "[RegionalRegistry] Regional '{}' ya registrado con ID {}",
                    name, info.region_id
                );
                return Ok((info.region_id, false));
            }
        }

        // Es nuevo: asignar ID
        let region_id = {
            let mut next_id = self.next_id.lock().unwrap();
            let id = *next_id;
            *next_id = next_id.wrapping_add(1); // Wrapping por si llegamos a 256
            id
        };

        let info = RegionalInfo {
            region_id,
            sync_address: sync_address.clone(),
        };

        // Guardar en ambos mapas
        {
            let mut regions = self.regions.write().unwrap();
            regions.insert(name.clone(), info.clone());
        }
        {
            let mut id_map = self.id_to_name.write().unwrap();
            id_map.insert(region_id, name.clone());
        }

        println!(
            "[RegionalRegistry] [INFO] Registered new regional '{}' with ID {} at {}",
            name, region_id, sync_address
        );

        Ok((region_id, true))
    }

    /// Obtiene el ID de una región por nombre
    pub fn get_region_id(&self, name: &str) -> Option<u8> {
        let regions = self.regions.read().unwrap();
        regions.get(name).map(|info| info.region_id)
    }

    /// Obtiene la dirección de sync de una región
    pub fn get_sync_address(&self, name: &str) -> Option<String> {
        let regions = self.regions.read().unwrap();
        regions.get(name).map(|info| info.sync_address.clone())
    }

    /// Lista todos los nombres de regiones registradas
    pub fn list_regions(&self) -> Vec<String> {
        let regions = self.regions.read().unwrap();
        regions.keys().cloned().collect()
    }

    /// Propaga un mensaje de sincronización al regional correspondiente
    /// No reintenta, solo intenta una vez (best effort)
    pub fn propagate(&self, region_name: &str, msg: CentralSyncMessage) {
        let address = match self.get_sync_address(region_name) {
            Some(addr) => addr,
            None => {
                eprintln!("[Propagate] Regional '{}' no registrado", region_name);
                return;
            }
        };

        // Intentar envío (sin retry)
        match TcpStream::connect(&address) {
            Ok(mut stream) => {
                let json = match serde_json::to_string(&msg) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!("[Propagate] Error serializando mensaje: {}", e);
                        return;
                    }
                };

                if stream.write_all(json.as_bytes()).is_ok()
                    && stream.write_all(b"\n").is_ok()
                    && stream.flush().is_ok()
                {
                    // Éxito (no loguear para no saturar)
                } else {
                    eprintln!(
                        "[Propagate] [ERROR] Failed sending to '{}' ({})",
                        region_name, address
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "[Propagate] [ERROR] Could not connect to '{}' ({}): {}",
                    region_name, address, e
                );
            }
        }
    }
}

/// Valida que un nombre de región sea alfanumérico (incluyendo guiones y guiones bajos)
fn is_valid_region_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_region_names() {
        assert!(is_valid_region_name("CABA"));
        assert!(is_valid_region_name("Buenos_Aires"));
        assert!(is_valid_region_name("Region-1"));
        assert!(is_valid_region_name("R123"));
    }

    #[test]
    fn test_invalid_region_names() {
        assert!(!is_valid_region_name(""));
        assert!(!is_valid_region_name("Buenos Aires")); // espacio
        assert!(!is_valid_region_name("CABA!"));
        assert!(!is_valid_region_name("Región#1"));
    }

    #[test]
    fn test_regional_registry_new() {
        let registry = RegionalRegistry::new();
        let regions = registry.regions.read().unwrap();
        
        assert_eq!(regions.len(), 0);
    }

    #[test]
    fn test_register_region_success() {
        let registry = RegionalRegistry::new();
        
        let result = registry.register("CABA".to_string(), "127.0.0.1:8080".to_string());
        
        assert!(result.is_ok());
        let (region_id, was_new) = result.unwrap();
        assert_ne!(region_id, 0); // Los IDs son válidos (no 0)
        assert!(was_new); // Primera vez que se registra
    }

    #[test]
    fn test_register_region_duplicate() {
        let registry = RegionalRegistry::new();
        
        let result1 = registry.register("CABA".to_string(), "127.0.0.1:8080".to_string());
        assert!(result1.is_ok());
        let (id1, was_new1) = result1.unwrap();
        assert!(was_new1);
        
        // Registrar de nuevo la misma región
        let result2 = registry.register("CABA".to_string(), "127.0.0.1:9090".to_string());
        assert!(result2.is_ok());
        let (id2, was_new2) = result2.unwrap();
        assert_eq!(id1, id2); // Mismo ID
        assert!(!was_new2); // No es nuevo
    }

    #[test]
    fn test_register_region_invalid_name() {
        let registry = RegionalRegistry::new();
        
        // Nombre vacío
        let result1 = registry.register("".to_string(), "127.0.0.1:8080".to_string());
        assert!(result1.is_err());
        
        // Nombre con caracteres inválidos
        let result2 = registry.register("CABA@2024".to_string(), "127.0.0.1:8080".to_string());
        assert!(result2.is_err());
        
        // Nombre con espacios
        let result3 = registry.register("CABA 1".to_string(), "127.0.0.1:8080".to_string());
        assert!(result3.is_err());
    }

    #[test]
    fn test_register_multiple_regions() {
        let registry = RegionalRegistry::new();
        
        let (id1, _) = registry.register("CABA".to_string(), "127.0.0.1:8080".to_string()).unwrap();
        let (id2, _) = registry.register("Cordoba".to_string(), "127.0.0.1:8081".to_string()).unwrap();
        let (id3, _) = registry.register("Mendoza".to_string(), "127.0.0.1:8082".to_string()).unwrap();
        
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
        
        let regions = registry.regions.read().unwrap();
        assert_eq!(regions.len(), 3);
    }

    #[test]
    fn test_get_region_id() {
        let registry = RegionalRegistry::new();
        
        let (id, _) = registry.register("CABA".to_string(), "127.0.0.1:8080".to_string()).unwrap();
        
        let retrieved_id = registry.get_region_id("CABA");
        assert_eq!(retrieved_id, Some(id));
        
        let not_found = registry.get_region_id("Inexistente");
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_get_sync_address() {
        let registry = RegionalRegistry::new();
        
        let address = "127.0.0.1:8080".to_string();
        registry.register("CABA".to_string(), address.clone()).unwrap();
        
        let retrieved_address = registry.get_sync_address("CABA");
        assert_eq!(retrieved_address, Some(address));
        
        let not_found = registry.get_sync_address("Inexistente");
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_list_all_regions() {
        let registry = RegionalRegistry::new();
        
        registry.register("CABA".to_string(), "127.0.0.1:8080".to_string()).unwrap();
        registry.register("Cordoba".to_string(), "127.0.0.1:8081".to_string()).unwrap();
        
        let regions = registry.regions.read().unwrap();
        assert_eq!(regions.len(), 2);
        assert!(regions.contains_key("CABA"));
        assert!(regions.contains_key("Cordoba"));
    }

    #[test]
    fn test_concurrent_registration() {
        use std::thread;
        
        let registry = Arc::new(RegionalRegistry::new());
        
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let reg = Arc::clone(&registry);
                thread::spawn(move || {
                    let region_name = format!("Region_{}", i);
                    let address = format!("127.0.0.1:808{}", i);
                    reg.register(region_name, address)
                })
            })
            .collect();
        
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.join().unwrap());
        }
        
        // Todas las registraciones deberían ser exitosas
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 10);
        
        // Todos los IDs deberían ser únicos
        let ids: Vec<u8> = results
            .iter()
            .filter_map(|r| r.as_ref().ok().map(|(id, _)| *id))
            .collect();
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique_ids.len());
    }
}
