#![allow(dead_code)]

// algol26/src/region.rs

use std::collections::HashMap;

/// Region-based memory management
/// Regions group allocations for bulk cleanup

#[derive(Debug, Clone)]
pub struct Region {
    pub id: usize,
    pub name: String,
    pub active: bool,
}

#[derive(Debug)]
pub struct RegionManager {
    regions: HashMap<usize, Region>,
    current_region: Option<usize>,
    next_id: usize,
}

impl RegionManager {
    pub fn new() -> Self {
        RegionManager {
            regions: HashMap::new(),
            current_region: None,
            next_id: 0,
        }
    }
    
    pub fn create_region(&mut self, name: &str) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.regions.insert(id, Region {
            id,
            name: name.to_string(),
            active: true,
        });
        id
    }
    
    pub fn enter_region(&mut self, id: usize) -> Result<(), String> {
        if let Some(region) = self.regions.get(&id) {
            if region.active {
                self.current_region = Some(id);
                Ok(())
            } else {
                Err(format!("Region '{}' is not active", region.name))
            }
        } else {
            Err(format!("Region {} not found", id))
        }
    }
    
    pub fn exit_region(&mut self) {
        self.current_region = None;
    }
    
    pub fn deallocate_region(&mut self, id: usize) -> Result<(), String> {
        if let Some(region) = self.regions.get_mut(&id) {
            region.active = false;
            Ok(())
        } else {
            Err(format!("Region {} not found", id))
        }
    }
    
    pub fn current_region(&self) -> Option<usize> {
        self.current_region
    }
    
    pub fn is_active(&self, id: usize) -> bool {
        self.regions.get(&id).map(|r| r.active).unwrap_or(false)
    }
}
