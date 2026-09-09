use std::collections::{HashMap, HashSet};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::alloc::{alloc, dealloc, Layout};

const ALIGNMENT: usize = 16;

#[derive(Debug)]
pub struct RegionAllocator {
    regions: HashMap<String, Region>,
    memory: HashMap<String, Vec<MemoryBlock>>,
    allocation_counter: AtomicUsize,
}

#[derive(Debug, Clone)]
pub struct Region {
    pub name: String,
    pub size: usize,
    pub used: usize,
    pub freed: bool,
    pub parent: Option<String>,
    pub children: HashSet<String>,
}

#[derive(Debug)]
struct MemoryBlock {
    ptr: NonNull<u8>,
    size: usize,
    layout: Layout,
    allocated: bool,
}

impl RegionAllocator {
    pub fn new() -> Self {
        RegionAllocator {
            regions: HashMap::new(),
            memory: HashMap::new(),
            allocation_counter: AtomicUsize::new(0),
        }
    }

    pub fn create_region(&mut self, name: &str) -> Result<(), String> {
        if self.regions.contains_key(name) {
            return Err(format!("Region '{}' already exists", name));
        }
        self.regions.insert(name.to_string(), Region {
            name: name.to_string(), size: 0, used: 0, freed: false,
            parent: None, children: HashSet::new(),
        });
        self.memory.insert(name.to_string(), Vec::new());
        Ok(())
    }

    pub fn create_child_region(&mut self, parent: &str, child: &str) -> Result<(), String> {
        if !self.regions.contains_key(parent) {
            return Err(format!("Parent region '{}' not found", parent));
        }
        if self.regions[parent].freed {
            return Err(format!("Parent region '{}' is freed", parent));
        }
        self.create_region(child)?;
        if let Some(p) = self.regions.get_mut(parent) {
            p.children.insert(child.to_string());
        }
        if let Some(c) = self.regions.get_mut(child) {
            c.parent = Some(parent.to_string());
        }
        Ok(())
    }

    pub fn allocate(&mut self, region_name: &str, size: usize) -> Result<NonNull<u8>, String> {
        let (freed, parent_opt) = {
            let r = self.regions.get(region_name).ok_or_else(|| format!("Region '{}' not found", region_name))?;
            (r.freed, r.parent.clone())
        };
        if freed { return Err(format!("Region '{}' is freed", region_name)); }
        if let Some(parent_name) = parent_opt {
            if let Some(p) = self.regions.get(&parent_name) {
                if p.freed { return Err(format!("Parent region '{}' is freed", parent_name)); }
            }
        }
        let layout = Layout::from_size_align(size, ALIGNMENT).map_err(|e| e.to_string())?;
        let raw = unsafe { alloc(layout) };
        let ptr = NonNull::new(raw).ok_or_else(|| "alloc failed".to_string())?;
        let block = MemoryBlock { ptr, size, layout, allocated: true };
        self.memory.get_mut(region_name).ok_or_else(|| format!("No memory entry for region '{}'", region_name))?.push(block);
        if let Some(r) = self.regions.get_mut(region_name) {
            r.size += layout.size();
            r.used += size;
        }
        self.allocation_counter.fetch_add(1, Ordering::SeqCst);
        Ok(ptr)
    }

    pub fn deallocate(&mut self, region_name: &str, ptr: NonNull<u8>) -> Result<(), String> {
        let region = self.regions.get(region_name).ok_or_else(|| format!("Region '{}' not found", region_name))?;
        if region.freed { return Err(format!("Region '{}' is already freed", region_name)); }
        let blocks = self.memory.get_mut(region_name).ok_or_else(|| format!("No memory blocks for region '{}'", region_name))?;
        if let Some(idx) = blocks.iter().position(|b| b.ptr == ptr && b.allocated) {
            let block = &mut blocks[idx];
            unsafe { dealloc(block.ptr.as_ptr(), block.layout); }
            block.allocated = false;
            if let Some(r) = self.regions.get_mut(region_name) {
                r.used = r.used.saturating_sub(block.size);
            }
            Ok(())
        } else {
            Err("Pointer not found or already deallocated".to_string())
        }
    }

    pub fn free_region(&mut self, region_name: &str) -> Result<(), String> {
        if !self.regions.contains_key(region_name) {
            return Err(format!("Region '{}' not found", region_name));
        }
        let children: Vec<String> = self.regions[region_name].children.iter().cloned().collect();
        for child in children { self.free_region(&child)?; }
        if let Some(blocks) = self.memory.get_mut(region_name) {
            for block in blocks.iter_mut().filter(|b| b.allocated) {
                unsafe { dealloc(block.ptr.as_ptr(), block.layout); }
                block.allocated = false;
            }
            blocks.clear();
        }
        if let Some(r) = self.regions.get_mut(region_name) {
            r.freed = true;
            r.used = 0;
        }
        Ok(())
    }

    pub fn is_freed(&self, region_name: &str) -> Option<bool> {
        self.regions.get(region_name).map(|r| r.freed)
    }

    pub fn is_valid_ptr(&self, region_name: &str, ptr: NonNull<u8>) -> bool {
        self.memory.get(region_name).map_or(false, |blocks| blocks.iter().any(|b| b.ptr == ptr && b.allocated))
    }

    pub fn total_allocated(&self) -> usize { self.regions.values().filter(|r| !r.freed).map(|r| r.size).sum() }
    pub fn total_used(&self) -> usize { self.regions.values().filter(|r| !r.freed).map(|r| r.used).sum() }
    pub fn total_allocations(&self) -> usize { self.allocation_counter.load(Ordering::SeqCst) }
    pub fn get_region_info(&self, name: &str) -> Option<&Region> { self.regions.get(name) }
}

impl Default for RegionAllocator { fn default() -> Self { Self::new() } }
impl Drop for RegionAllocator {
    fn drop(&mut self) {
        let names: Vec<String> = self.regions.keys().cloned().collect();
        for n in names { let _ = self.free_region(&n); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_create_and_allocate() {
        let mut a = RegionAllocator::new(); a.create_region("test").unwrap();
        let ptr = a.allocate("test", 100).unwrap(); assert!(a.is_valid_ptr("test", ptr));
    }
    #[test] fn test_double_create_fails() {
        let mut a = RegionAllocator::new(); a.create_region("test").unwrap();
        assert!(a.create_region("test").is_err());
    }
    #[test] fn test_allocate_in_freed_region_fails() {
        let mut a = RegionAllocator::new(); a.create_region("test").unwrap();
        a.free_region("test").unwrap(); assert!(a.allocate("test", 100).is_err());
    }
    #[test] fn test_double_free_prevented() {
        let mut a = RegionAllocator::new(); a.create_region("test").unwrap();
        let ptr = a.allocate("test", 100).unwrap(); a.deallocate("test", ptr).unwrap();
        assert!(a.deallocate("test", ptr).is_err());
    }
    #[test] fn test_child_region_management() {
        let mut a = RegionAllocator::new(); a.create_region("parent").unwrap();
        a.create_child_region("parent", "child").unwrap(); a.free_region("parent").unwrap();
        assert_eq!(a.is_freed("child"), Some(true));
    }
}
