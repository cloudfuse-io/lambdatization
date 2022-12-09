# L12N CLI image

This image provides an "all in one" environment to run the various operations
that are defined by the L12N CLI:
- it can be executed locally by running the `l12n-shell` or inside lambda using
  the `lambdacli` module
- it takes care of both pinning dependencies and setting the right environment
  variables with sensible defaults

## Configuration

The following environment variables are expected to be configured in the final
runtime:
- `REPO_DIR` the root directory of the repository
- `CALLING_DIR` the current working directory when calling this script

If using the `cli` target (or `entrypoint.sh` in general), also provide:
- `HOST_UID` user ID of the host system caller
- `HOST_GID` group ID of the host system caller
- `HOST_DOCKER_GID` group ID assigned to the docker socket
