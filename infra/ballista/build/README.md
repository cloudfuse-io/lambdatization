# Ballista lambdatization tricks

## List of tricks

- We clone the arrow-ballista repo for a given version.
- Since there are not official docker images for ballista, but, inside the repo
  there are Dockerfiles for each part (builder, scheduler, executor) we've merged
  this images into one Dockerfile and passed the entry points into the lambda
  function.
- We use the Python AWS Lambda Runtime Interface Client, but it requires adding
  Python to the base image. A Rust based Interface Client would spare us a few
  dozen MBs.
- We execute the query using ballista-cli
- the default config for the scheduler is on standalone using sled. Sled default
  directory is set to /dev/shm wich is not available in lambda. In order to cover
  for this we use the inline parameter --sled-dir to change the sled directory
  towards /tmp/scheduler (/tmp being the only writable dir on lambda env)
- For the excecutor the default working dir is on /tmp, when we try to change it
  an internal tmp folder /tmp/executor. The executor failed to start with error:
  `Failed to init Executor RuntimeEnv`
