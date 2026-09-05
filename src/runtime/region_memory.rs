// ALGOL26 - Region Memory (Controlled Memory Layer)
// Provides arena-based deterministic memory management

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RegionAllocator {
    regions: HashMap<String, Region>,
}

#[derive(Debug, Clone)]
pub struct Region {
    pub name: String,
    pub size: usize,
    pub used: usize,
    pub freed: bool,
}

impl RegionAllocator {
    pub fn new() -> Self {
        RegionAllocator {
            regions: HashMap::new(),
        }
    }

    pub fn create_region(&mut self, name: &str) {
        self.regions.insert(
            name.to_string(),
            Region {
                name: name.to_string(),
                size: 0,
                used: 0,
                freed: false,
            },
        );
    }

    pub fn allocate(&mut self, region_name: &str, size: usize) -> bool {
        if let Some(region) = self.regions.get_mut(region_name) {
            if !region.freed {
                region.size += size;
                region.used += size;
                return true;
            }
        }
        false
    }

    pub fn free_region(&mut self, region_name: &str) {
        if let Some(region) = self.regions.get_mut(region_name) {
            region.freed = true;
            region.used = 0;
        }
    }

    pub fn is_freed(&self, region_name: &str) -> bool {
        self.regions
            .get(region_name)
            .map(|r| r.freed)
            .unwrap_or(false)
    }

    pub fn total_allocated(&self) -> usize {
        self.regions.values().map(|r| r.size).sum()
    }

    pub fn total_used(&self) -> usize {
        self.regions.values().map(|r| r.used).sum()
    }
}

impl Default for RegionAllocator {
    fn default() -> Self {
        Self::new()
    }
}
