use crate::utils::{read_string, read_u64};
use std::io::Cursor;

/// Constante hecha para que el admin mande la solicitud.
pub const PROBE: u8 = 7;

/// Mensaje hecho para que el servidor central pueda responder
/// al administrador qué nodo es el líder.
pub struct ProbeResponse {
    leader_ip: String,
    known_nodes: Vec<String>,
}

impl ProbeResponse {
    pub fn new(leader_ip: String, known_nodes: Vec<String>) -> Self {
        Self {
            leader_ip,
            known_nodes,
        }
    }

    pub fn ip(&self) -> String {
        self.leader_ip.to_string()
    }

    pub fn nodes(&self) -> Vec<String> {
        self.known_nodes.clone()
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        let ip_bytes = self.leader_ip.as_bytes();
        let ip_bytes_len = ip_bytes.len() as u64;

        buf.extend_from_slice(&ip_bytes_len.to_be_bytes());
        buf.extend_from_slice(ip_bytes);

        let nodes_len = self.known_nodes.len() as u64;
        buf.extend_from_slice(&nodes_len.to_be_bytes());

        for node in &self.known_nodes {
            let bytes_aux = node.as_bytes();
            let bytes_aux_len = bytes_aux.len() as u64;

            buf.extend_from_slice(&bytes_aux_len.to_be_bytes());
            buf.extend_from_slice(bytes_aux);
        }
        //println!("probe res a mandar {:?}", buf);
        buf
    }

    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(&mut bytes);

        let size = read_u64(&mut cursor)?;
        let leader_ip = read_string(&mut cursor, size as usize)?;

        let known_nodes_len = read_u64(&mut cursor)?;
        let mut known_nodes = Vec::new();

        for _ in 0..known_nodes_len {
            let aux_len = read_u64(&mut cursor)? as usize;
            let node_id = read_string(&mut cursor, aux_len)?;
            known_nodes.push(node_id);
        }

        Ok(Self {
            leader_ip,
            known_nodes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_probe_response_correctly() {
        let leader = "192.168.1.1:8080".to_string();
        let nodes = vec![
            "192.168.1.2:8080".to_string(),
            "192.168.1.3:8080".to_string(),
        ];

        let response = ProbeResponse::new(leader.clone(), nodes.clone());

        assert_eq!(response.ip(), leader);
        assert_eq!(response.nodes(), nodes);
    }

    #[test]
    fn test_ip_returns_leader_ip() {
        let leader = "10.0.0.1:9000".to_string();
        let response = ProbeResponse::new(leader.clone(), vec![]);

        assert_eq!(response.ip(), leader);
    }

    #[test]
    fn test_nodes_returns_known_nodes() {
        let nodes = vec![
            "node1:8080".to_string(),
            "node2:8080".to_string(),
            "node3:8080".to_string(),
        ];
        let response = ProbeResponse::new("leader:8080".to_string(), nodes.clone());

        assert_eq!(response.nodes(), nodes);
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let leader = "192.168.1.100:8080".to_string();
        let nodes = vec![
            "192.168.1.101:8080".to_string(),
            "192.168.1.102:8080".to_string(),
        ];
        let original = ProbeResponse::new(leader, nodes);

        let serialized = original.serialize();
        let deserialized =
            ProbeResponse::deserialize(&serialized).expect("Deserialization should succeed");

        assert_eq!(original.ip(), deserialized.ip());
        assert_eq!(original.nodes(), deserialized.nodes());
    }

    #[test]
    fn test_serialize_with_empty_nodes() {
        let leader = "localhost:8080".to_string();
        let response = ProbeResponse::new(leader.clone(), vec![]);

        let serialized = response.serialize();
        let deserialized =
            ProbeResponse::deserialize(&serialized).expect("Deserialization should succeed");

        assert_eq!(deserialized.ip(), leader);
        assert_eq!(deserialized.nodes(), Vec::<String>::new());
    }

    #[test]
    fn test_serialize_with_single_node() {
        let leader = "leader.example.com:8080".to_string();
        let nodes = vec!["node.example.com:8080".to_string()];
        let response = ProbeResponse::new(leader.clone(), nodes.clone());

        let serialized = response.serialize();
        let deserialized =
            ProbeResponse::deserialize(&serialized).expect("Deserialization should succeed");

        assert_eq!(deserialized.ip(), leader);
        assert_eq!(deserialized.nodes(), nodes);
    }

    #[test]
    fn test_serialize_with_many_nodes() {
        let leader = "master:8080".to_string();
        let nodes: Vec<String> = (1..=10).map(|i| format!("node{}:8080", i)).collect();
        let response = ProbeResponse::new(leader.clone(), nodes.clone());

        let serialized = response.serialize();
        let deserialized =
            ProbeResponse::deserialize(&serialized).expect("Deserialization should succeed");

        assert_eq!(deserialized.ip(), leader);
        assert_eq!(deserialized.nodes(), nodes);
    }

    #[test]
    fn test_serialize_with_special_characters() {
        let leader = "leader-node_1.local:8080".to_string();
        let nodes = vec![
            "node-1_test.local:8080".to_string(),
            "node-2_test.local:9090".to_string(),
        ];
        let response = ProbeResponse::new(leader.clone(), nodes.clone());

        let serialized = response.serialize();
        let deserialized =
            ProbeResponse::deserialize(&serialized).expect("Deserialization should succeed");

        assert_eq!(deserialized.ip(), leader);
        assert_eq!(deserialized.nodes(), nodes);
    }

    #[test]
    fn test_deserialize_empty_buffer_fails() {
        let empty_buffer: Vec<u8> = vec![];
        let result = ProbeResponse::deserialize(&empty_buffer);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_incomplete_buffer_fails() {
        let incomplete_buffer = vec![0, 0, 0, 0]; // Only 4 bytes, not enough for u64
        let result = ProbeResponse::deserialize(&incomplete_buffer);

        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_format_structure() {
        let leader = "test:8080".to_string();
        let nodes = vec!["node1:8080".to_string()];
        let response = ProbeResponse::new(leader.clone(), nodes.clone());

        let serialized = response.serialize();

        // Check that buffer starts with leader IP length (u64 = 8 bytes)
        assert!(serialized.len() >= 8);

        // Verify the first 8 bytes represent the leader IP length
        let ip_len = u64::from_be_bytes(serialized[0..8].try_into().unwrap());
        assert_eq!(ip_len, leader.len() as u64);
    }

    #[test]
    fn test_nodes_returns_clone() {
        let nodes = vec!["node1:8080".to_string()];
        let response = ProbeResponse::new("leader:8080".to_string(), nodes.clone());

        let nodes1 = response.nodes();
        let nodes2 = response.nodes();

        // Both should be equal but separate clones
        assert_eq!(nodes1, nodes2);
        assert_eq!(nodes1, nodes);
    }

    #[test]
    fn test_ip_returns_clone() {
        let leader = "leader:8080".to_string();
        let response = ProbeResponse::new(leader.clone(), vec![]);

        let ip1 = response.ip();
        let ip2 = response.ip();

        // Both should be equal but separate clones
        assert_eq!(ip1, ip2);
        assert_eq!(ip1, leader);
    }

    #[test]
    fn test_serialize_with_unicode_characters() {
        let leader = "líder:8080".to_string();
        let nodes = vec!["nodo1:8080".to_string(), "节点2:8080".to_string()];
        let response = ProbeResponse::new(leader.clone(), nodes.clone());

        let serialized = response.serialize();
        let deserialized =
            ProbeResponse::deserialize(&serialized).expect("Deserialization should succeed");

        assert_eq!(deserialized.ip(), leader);
        assert_eq!(deserialized.nodes(), nodes);
    }

    #[test]
    fn test_serialize_with_very_long_strings() {
        let leader = "a".repeat(1000);
        let nodes = vec!["b".repeat(500), "c".repeat(500)];
        let response = ProbeResponse::new(leader.clone(), nodes.clone());

        let serialized = response.serialize();
        let deserialized =
            ProbeResponse::deserialize(&serialized).expect("Deserialization should succeed");

        assert_eq!(deserialized.ip(), leader);
        assert_eq!(deserialized.nodes(), nodes);
    }
}
