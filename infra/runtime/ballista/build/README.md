# Ballista lambdatization tricks

## List of tricks

- Since there are not official docker images for ballista, we provide our own
  build of Ballista with the project's CI.
- the default config for the scheduler is on standalone using sled. Sled default
  directory is set to /dev/shm wich is not available in lambda. In order to cover
  for this we use the inline parameter --sled-dir to change the sled directory
  towards /tmp/scheduler (/tmp being the only writable dir on lambda env)
- For the excecutor the default working dir is on /tmp, when we try to change it
  an internal tmp folder /tmp/executor. The executor failed to start with error:
  `Failed to init Executor RuntimeEnv`
