# Dask lambdatization tricks

## Tricks

- We needed to disable "nanny" (`--no-nanny`) because it's using
  `multiprocessing` features that don't work on Lambda (because of missing
  `/dev/shm`)
