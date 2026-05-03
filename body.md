- [x] Configurable constants: PAGE_SIZE (8192), BUFFER_POOL_SIZE (number of frames), SEGMENT_SIZE (1GB by default), LRU_CAPACITY.
- [x] Validate that PAGE_SIZE is a multiple of the sector size (512/4096).
- [x] Load from config.toml in the database directory.

closes #5
