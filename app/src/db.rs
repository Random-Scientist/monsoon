use std::{collections::HashMap, marker::PhantomData, path::Path};

use bincode::{Decode, Encode};
use log::error;
use rand::random;
use sled::Tree;

use crate::{show::Show, show::ShowId};
#[allow(unused)]
pub struct MainDb {
    db: sled::Db,
    pub shows: TypedTree<ShowId, Show>,
}

pub struct TypedTree<K: Into<u64> + From<u64> + Copy, V: Encode + Decode<()>> {
    _tys: PhantomData<(K, V)>,
    cache: HashMap<u64, V>,
    tree: Tree,
}

impl<K: Into<u64> + From<u64> + Copy, V: Encode + Decode<()>> TypedTree<K, V> {
    pub(crate) fn new(tree: Tree) -> Self {
        let mut cache = HashMap::with_capacity(tree.len());
        for v in tree.iter() {
            match v {
                Ok((k, v)) => {
                    let k = u64::from_le_bytes(
                        *k.first_chunk::<8>()
                            .expect("keys to be at least 4 bytes long"),
                    );
                    let (v, _) = bincode::decode_from_slice(&v, bincode::config::standard())
                        .expect("show to be well formed");
                    cache.insert(k, v);
                }
                Err(e) => error!("failed to load a show from the database: {e}"),
            }
        }
        Self {
            _tys: PhantomData,
            cache,
            tree,
        }
    }

    pub fn enumerate(&self) -> impl Iterator<Item = (K, &V)> {
        self.cache.iter().map(|(a, b)| (K::from(*a), b))
    }

    pub fn get(&self, id: K) -> Option<&V> {
        self.cache.get(&id.into())
    }

    pub fn update_with<R>(&mut self, key: K, f: impl FnOnce(&mut V) -> R) -> Option<R> {
        let r = Some(f(self.cache.get_mut(&key.into())?));
        self.flush(key);
        r
    }

    /// force-synchronizes the persisted representation for a given ShowId to the current cached one
    pub fn flush(&mut self, id: K) {
        let k = id.into();
        if let Some(s) = self.cache.get(&k) {
            self.tree
                .insert(
                    k.to_le_bytes(),
                    bincode::encode_to_vec(s, bincode::config::standard())
                        .expect("serialization to succeed"),
                )
                .expect("database write to succeed");
        }
    }

    /// Same as [`Self::flush`] but for every item in the cache
    pub fn flush_all(&mut self) {
        for (k, v) in self.cache.iter() {
            self.tree
                .insert(
                    k.to_le_bytes(),
                    bincode::encode_to_vec(v, bincode::config::standard())
                        .expect("serialization to succeed"),
                )
                .expect("database write to succeed");
        }
    }

    pub fn insert(&mut self, value: V) -> K {
        let id = self.available_id();
        self.write(id, &value);
        let _ = self.cache.insert(id, value);
        id.into()
    }

    pub fn drop(&mut self, id: K) -> Option<V> {
        let k = id.into();
        let val = self.cache.remove(&k)?;
        let _ = self
            .tree
            .remove(k.to_le_bytes())
            .expect("DB item removal to succeed");
        Some(val)
    }

    fn write(&self, k: u64, v: &V) {
        self.tree
            .insert(
                k.to_le_bytes(),
                bincode::encode_to_vec(v, bincode::config::standard())
                    .expect("serialization to succeed"),
            )
            .expect("database write to succeed");
    }

    fn available_id(&self) -> u64 {
        let mut cand;
        loop {
            cand = random::<u64>();
            // guarantees we will eventually expose the entire 64 bit keyspace
            if self
                .tree
                .get(cand.to_le_bytes())
                .expect("db access to succeed")
                .is_none()
            {
                break;
            }
        }
        cand
    }
}

impl<K: Into<u64> + From<u64> + Copy, V: Encode + Decode<()>> Drop for TypedTree<K, V> {
    fn drop(&mut self) {
        self.flush_all();
    }
}

impl MainDb {
    pub(crate) fn open(p: impl AsRef<Path>) -> Self {
        let db = sled::open(p).expect("database to open");
        let shows = db.open_tree(b"shows").expect("shows tree to open");
        let shows = TypedTree::new(shows);
        Self { db, shows }
    }
}
