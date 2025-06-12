use std::marker::PhantomData;

use rocksdb::ReadOptions;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KvStoreError {
    #[error("Feature id not found")]
    NotFound,
    #[error("Ill formed values {0}")]
    IllFormed(#[from] serde_json::Error),
    #[error("RocksDB error: {0}")]
    DbError(#[from] rocksdb::Error),
}

pub struct Secondary;
pub struct Primary;

pub struct KvStore<M> {
    db_handle: rocksdb::DB,
    read_opts: ReadOptions,
    _mode: PhantomData<M>,
}

impl<M> KvStore<M> {
    fn create_opts(cache_size: usize) -> rocksdb::Options {
        let mut opts = rocksdb::Options::default();
        opts.set_max_background_jobs(0);
        opts.set_max_subcompactions(0);
        opts.set_disable_auto_compactions(true);
        opts.set_use_direct_reads(false);
        opts.set_use_direct_io_for_flush_and_compaction(false);

        opts.set_max_open_files(1024);  

        opts.optimize_for_point_lookup(32); 

        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        // Increase cache sizes
        let cache = rocksdb::Cache::new_lru_cache(cache_size);
        opts.set_row_cache(&cache);
        let mut block_opts = rocksdb::BlockBasedOptions::default();
        block_opts.set_block_size(4 * 1024);           // 4 KB blocks
        // allocate LRU cache for hot blocks
        // fine‑tune bloom filters to avoid disk seeks
        block_opts.set_bloom_filter(10., true);          // 10 bits/key, optimize for hit
        block_opts.set_block_cache(&cache);
        opts.set_block_based_table_factory(&block_opts);
        opts
    }
}

impl KvStore<Secondary> {
    pub fn try_new_secondary(
        db_path: &str,
        db_sec_path: &str,
        cache_size: usize,
    ) -> Result<Self, KvStoreError> {
        let opts = KvStore::<Secondary>::create_opts(cache_size);
        let db_handle = rocksdb::DB::open_as_secondary(&opts, db_path, db_sec_path)?;
        let mut read_opts = rocksdb::ReadOptions::default();
        read_opts.set_verify_checksums(false); // Skip if data integrity handled elsewhere
        read_opts.fill_cache(true);
        Ok(KvStore::<Secondary> {
            db_handle,
            read_opts,
            _mode: PhantomData::<Secondary>,
        })
    }

    pub fn get_feature(&self, key: &str) -> Result<common::IdFeatures, KvStoreError> {

        let value = self.db_handle.get_opt(key, &self.read_opts)?.ok_or(KvStoreError::NotFound)?;
        Ok(serde_json::from_slice::<common::IdFeatures>(&value)?)
    }
    pub fn catchup(&self) -> Result<(), KvStoreError> {
        Ok(self.db_handle.try_catch_up_with_primary()?)
    }
}

impl KvStore<Primary> {
    pub fn try_new_primary(db_path: &str, cache_size: usize) -> Result<Self, KvStoreError> {
        let mut opts = KvStore::<Primary>::create_opts(cache_size);
        opts.create_if_missing(true);
        let db_handle = rocksdb::DB::open(&opts, db_path)?;
        Ok(KvStore::<Primary> {
            db_handle,
            read_opts: ReadOptions::default(),
            _mode: PhantomData::<Primary>,
        })
    }

    pub fn write_features(&self, slice: &[(&str, common::IdFeatures)]) -> Result<(), KvStoreError> {
        let mut batch = rocksdb::WriteBatch::default();
        for (key, value) in slice {
            // generate features
            let value = serde_json::to_vec(&value)?;
            batch.put(key.as_bytes(), &value);
        }
        self.db_handle.write(batch)?;
        Ok(())
    }

    pub fn finalize_writes(&self) {
        self.db_handle.compact_range(None::<&[u8]>, None::<&[u8]>)
    }
}
