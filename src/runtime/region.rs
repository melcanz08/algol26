#![allow(dead_code)]

// algol26/src/runtime/region.rs
// HARDENED: Region-based memory management with proper stack discipline

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Region {
    pub id: usize,
    pub name: String,
    pub active: bool,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub allocations: usize,
    pub bytes_allocated: usize,
}

#[derive(Debug)]
pub struct RegionManager {
    regions: HashMap<usize, Region>,
    region_stack: Vec<usize>,
    next_id: usize,
    total_allocations: usize,
    total_bytes: usize,
}

impl RegionManager {
    pub fn new() -> Self {
        RegionManager {
            regions: HashMap::new(),
            region_stack: Vec::new(),
            next_id: 0,
            total_allocations: 0,
            total_bytes: 0,
        }
    }

    pub fn create_region(&mut self, name: &str) -> usize {
        let id = self.next_id;
        self.next_id += 1;

        self.regions.insert(
            id,
            Region {
                id,
                name: name.to_string(),
                active: true,
                parent: self.region_stack.last().copied(),
                children: Vec::new(),
                allocations: 0,
                bytes_allocated: 0,
            },
        );

        // Add as child to parent
        if let Some(parent_id) = self.region_stack.last().copied() {
            if let Some(parent) = self.regions.get_mut(&parent_id) {
                parent.children.push(id);
            }
        }

        id
    }

    pub fn enter_region(&mut self, id: usize) -> Result<(), String> {
        let region = self
            .regions
            .get(&id)
            .ok_or_else(|| format!("Region {} not found", id))?;

        if !region.active {
            return Err(format!("Region '{}' is not active", region.name));
        }

        // Check if parent is on stack (nested regions must follow stack discipline)
        if let Some(parent_id) = region.parent {
            if !self.region_stack.contains(&parent_id) {
                return Err(format!(
                    "Region '{}' has parent {} which is not on the stack",
                    region.name, parent_id
                ));
            }
        }

        // Check if this region is already on the stack
        if self.region_stack.contains(&id) {
            return Err(format!("Region '{}' is already on the stack", region.name));
        }

        self.region_stack.push(id);
        Ok(())
    }

    pub fn exit_region(&mut self) -> Result<usize, String> {
        self.region_stack
            .pop()
            .ok_or_else(|| "No active region to exit".to_string())
    }

    pub fn allocate_in_current(&mut self, size: usize) -> Result<(), String> {
        let region_id = self
            .region_stack
            .last()
            .ok_or_else(|| "No active region".to_string())?
            .clone();

        self.allocate_in_region(region_id, size)
    }

    pub fn allocate_in_region(&mut self, id: usize, size: usize) -> Result<(), String> {
        let region = self
            .regions
            .get_mut(&id)
            .ok_or_else(|| format!("Region {} not found", id))?;

        if !region.active {
            return Err(format!("Region '{}' is not active", region.name));
        }

        region.allocations += 1;
        region.bytes_allocated += size;

        self.total_allocations += 1;
        self.total_bytes += size;

        Ok(())
    }

    pub fn deallocate_region(&mut self, id: usize) -> Result<(), String> {
        // Check if region is on stack - cannot deallocate active region
        if self.region_stack.contains(&id) {
            return Err("Cannot deallocate active region".to_string());
        }

        let children: Vec<usize> = {
            let region = self
                .regions
                .get(&id)
                .ok_or_else(|| format!("Region {} not found", id))?;

            if !region.active {
                return Err(format!("Region '{}' is already deallocated", region.name));
            }

            region.children.clone()
        };

        // Deallocate children first
        for child_id in children {
            self.deallocate_region(child_id)?;
        }

        // Deallocate this region
        if let Some(region) = self.regions.get_mut(&id) {
            region.active = false;

            self.total_allocations -= region.allocations;
            self.total_bytes -= region.bytes_allocated;

            region.allocations = 0;
            region.bytes_allocated = 0;
        }

        Ok(())
    }

    pub fn deallocate_all(&mut self) {
        let region_ids: Vec<usize> = self.regions.keys().cloned().collect();

        // Deallocate from deepest to shallowest
        let mut sorted_ids = region_ids;
        sorted_ids.sort_by_key(|id| {
            let depth = self.get_region_depth(*id);
            std::cmp::Reverse(depth)
        });

        for id in sorted_ids {
            let _ = self.deallocate_region(id);
        }

        self.region_stack.clear();
    }

    fn get_region_depth(&self, id: usize) -> usize {
        let mut depth = 0;
        let mut current = Some(id);

        while let Some(region_id) = current {
            if let Some(region) = self.regions.get(&region_id) {
                depth += 1;
                current = region.parent;
            } else {
                break;
            }
        }

        depth
    }

    pub fn current_region(&self) -> Option<usize> {
        self.region_stack.last().copied()
    }

    pub fn is_active(&self, id: usize) -> bool {
        self.regions.get(&id).map(|r| r.active).unwrap_or(false)
    }

    pub fn get_region(&self, id: usize) -> Option<&Region> {
        self.regions.get(&id)
    }

    pub fn active_region_count(&self) -> usize {
        self.regions.values().filter(|r| r.active).count()
    }

    pub fn total_allocations(&self) -> usize {
        self.total_allocations
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub fn verify_stack_discipline(&self) -> Result<(), String> {
        // Verify that region stack follows proper LIFO order
        if self.region_stack.is_empty() {
            return Ok(());
        }

        for (i, &region_id) in self.region_stack.iter().enumerate() {
            if let Some(region) = self.regions.get(&region_id) {
                if !region.active {
                    return Err(format!("Region '{}' on stack is inactive", region.name));
                }

                // Check parent-child relationship
                if i > 0 {
                    let parent_id = self.region_stack[i - 1];
                    if region.parent != Some(parent_id) {
                        return Err(format!(
                            "Region stack violation: '{}' should have parent {} but has {:?}",
                            region.name, parent_id, region.parent
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for RegionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RegionManager {
    fn drop(&mut self) {
        // Clean up all regions on drop
        self.deallocate_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_enter_region() {
        let mut manager = RegionManager::new();
        let id = manager.create_region("test");
        assert!(manager.enter_region(id).is_ok());
        assert_eq!(manager.current_region(), Some(id));
    }

    #[test]
    fn test_nested_regions() {
        let mut manager = RegionManager::new();

        let outer = manager.create_region("outer");
        manager.enter_region(outer).unwrap();

        let inner = manager.create_region("inner");
        manager.enter_region(inner).unwrap();

        assert_eq!(manager.current_region(), Some(inner));
        assert_eq!(manager.get_region(inner).unwrap().parent, Some(outer));

        manager.exit_region().unwrap();
        manager.exit_region().unwrap();

        assert!(manager.verify_stack_discipline().is_ok());
    }

    #[test]
    fn test_deallocate_region() {
        let mut manager = RegionManager::new();
        let id = manager.create_region("test");

        manager.enter_region(id).unwrap();
        manager.allocate_in_current(100).unwrap();
        manager.exit_region().unwrap();

        manager.deallocate_region(id).unwrap();
        assert!(!manager.is_active(id));
    }

    #[test]
    fn test_cannot_deallocate_active_region() {
        let mut manager = RegionManager::new();
        let id = manager.create_region("test");

        manager.enter_region(id).unwrap();
        assert!(manager.deallocate_region(id).is_err());
    }

    #[test]
    fn test_cannot_enter_inactive_region() {
        let mut manager = RegionManager::new();
        let id = manager.create_region("test");

        manager.deallocate_region(id).unwrap();
        assert!(manager.enter_region(id).is_err());
    }

    #[test]
    fn test_stack_discipline_enforced() {
        let mut manager = RegionManager::new();

        let outer = manager.create_region("outer");
        manager.enter_region(outer).unwrap();

        // Create inner WHILE outer is on stack (establish parent)
        let inner = manager.create_region("inner");
        manager.exit_region().unwrap(); // Exit outer

        // Now try to enter inner - should FAIL because parent (outer) is not on stack
        assert!(manager.enter_region(inner).is_err());

        // Enter in correct order: outer first, then inner
        manager.enter_region(outer).unwrap();
        manager.enter_region(inner).unwrap();

        // Exit in correct order: inner first, then outer
        manager.exit_region().unwrap(); // Exit inner
        assert!(manager.exit_region().is_ok()); // Exit outer
    }

    #[test]
    fn test_child_regions_deallocated_with_parent() {
        let mut manager = RegionManager::new();

        let parent = manager.create_region("parent");
        manager.enter_region(parent).unwrap();

        let child = manager.create_region("child");
        manager.enter_region(child).unwrap();
        manager.exit_region().unwrap();
        manager.exit_region().unwrap();

        manager.deallocate_region(parent).unwrap();

        assert!(!manager.is_active(parent));
        assert!(!manager.is_active(child));
    }
}
