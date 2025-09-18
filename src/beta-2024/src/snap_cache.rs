use move_package::resolution::resolution_graph::ResolvedGraph;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::io::SeekFrom;
use std::sync::Mutex;
use vfs::{impls::memory::MemoryFS, VfsPath};

const MAX_SNAPSHOTS: usize = 4;

struct SnapshotCache {
    map: HashMap<String, VfsPath>,
    lru: VecDeque<String>,
}

static SNAP_CACHE: Lazy<Mutex<SnapshotCache>> = Lazy::new(|| {
    Mutex::new(SnapshotCache {
        map: HashMap::new(),
        lru: VecDeque::new(),
    })
});

pub fn ensure_snapshot_for_graph(
    resolved_graph: &ResolvedGraph,
    ide_files_root: &VfsPath,
) -> anyhow::Result<VfsPath> {
    // compute deps_hash and collect all source files
    let (deps_hash, files) = {
        let mut hasher = Sha256::new();
        let mut files = Vec::new();
        for rpkg in resolved_graph.package_table.values() {
            let is_dep = rpkg.package_path != resolved_graph.graph.root_path;
            for f in rpkg.get_sources(&resolved_graph.build_options).unwrap() {
                let fname = dunce::canonicalize(f.as_str())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| f.to_string());
                let content = match ide_files_root.join(&fname).and_then(|p| p.open_file()) {
                    Ok(mut vf) => {
                        let mut s = String::new();
                        let _ = vf.read_to_string(&mut s);
                        vf.seek(SeekFrom::Start(0))?;
                        s
                    }
                    Err(_) => std::fs::read_to_string(&fname)?,
                };
                if is_dep {
                    hasher.update(content.as_bytes());
                }
                files.push((fname, content));
            }
        }
        (format!("{:X}", hasher.finalize()), files)
    };

    // try get snapshot from cache with lock
    {
        let mut cache = SNAP_CACHE.lock().unwrap();
        if let Some(snap) = cache.map.get(&deps_hash).cloned() {
            eprintln!("Snapshot hit!");
            // update LRU
            if let Some(pos) = cache.lru.iter().position(|x| x == &deps_hash) {
                cache.lru.remove(pos);
            }
            cache.lru.push_back(deps_hash.clone());
            return Ok(snap.clone());
        }

        eprintln!("Snapshot miss, creating new one for {}...", deps_hash);
        // create new snapshot
        let top = VfsPath::new(MemoryFS::new());
        for (fname, content) in files {
            let p = top.join(&fname).unwrap();
            let _ = p.parent().create_dir_all();
            let mut f = p.create_file().unwrap();
            eprintln!("snapshot add file: {}", fname);
            let _ = f.write_all(content.as_bytes());
        }

        // insert or update cache
        cache.map.insert(deps_hash.clone(), top.clone());
        cache.lru.push_back(deps_hash.clone());
        while cache.map.len() > MAX_SNAPSHOTS {
            if let Some(old) = cache.lru.pop_front() {
                cache.map.remove(&old);
            }
        }
        Ok(top)
    }
}
