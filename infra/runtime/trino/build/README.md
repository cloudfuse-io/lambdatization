# Trino lambdatization tricks

## List of tricks

- Trino loads many plugins by default, which implies opening many jar files in
  parallel. To make sure this process doesn't exceed the system's maximum number
  of file descriptors, it performs a check of the ulimit when starting. The
  minimum required is 4096, but unfortunately we have a hard limit on AWS Lambda
  at 1024. We had to [rebuild][trino_action] Trino with a patch that:
    - loads less plugins
    - removes the check on fileno
- Trino, like Dremio, automatically detects its private IP and tries to use it
  for internal connections. We didn't find a knob to disable this behaviour, so
  we had to harcode it in the patch.
- It seems you cannot query S3 without using the Hive metastore, so we had to
  install a local version of it running on Derby which adds to the init time.
- The container image is huge (>2GB):
  - we are pulling in a full Hadoop distribution, in which most files won't be
    used. We started removing some libraries from it but we could probably trim
    a few more hundreds of MBs
  - we could also use a remote Hive metastore (like Glue) instead of installing
    a local one
  - obviously, we could use a smaller base image

[trino_action]: https://github.com/cloudfuse-io/lambdatization/actions/workflows/helper-trino.yaml

## Updating Trino version

To change the Trino version, the patch needs to be applied to that version (xxx):
```bash
git clone cloudfuse-io/trino
cd trino
git checkout 378-patch
git checkout -b xxx-patch
git rebase xxx
git push
```

Then run the build in the [Trino workflow][trino_workflow] with your new Trino
version number xxx

[trino_workflow]: https://github.com/cloudfuse-io/lambdatization/actions/workflows/helper-trino.yaml
