# ClickHouse lambdatization tricks

## List of tricks

- Clickhouse offers an Alpine version of its image, but we favor the Ubuntu one
  as we also used Debian/Ubuntu for the other images. The Alpine image is only
  30MB smaller so the gain wouldn't be huge anyway.
- Lambda does not support `prctl` with the `PR_SET_NAME` flag. We provide a
  [custom build](/.github/workflows/helper-clickhouse.yaml) that doesn't raise
  an exception when that call fails. To create a new image:
  - setup an [Ubuntu 20.04 runner](/.github/gh-runner-setup.sh)
  - create a branch in [cloudfuse-io/ClickHouse][cloudfuse_clickhouse_fork] with
    the patch
  - run the ClickHouse [build action][clickhouse_build_action]
- For some reason, `libunwind` doesn't seem to work on lambda. In the custom
  build we tried to disable it (`-DUSE_UNWIND=0`), but then CMake didn't have
  libgcc_eh available, so we force link to it instead
  
```
target_link_libraries(cxxabi PUBLIC /usr/lib/gcc/x86_64-linux-gnu/9/libgcc_eh.a)
```

[cloudfuse_clickhouse_fork]: https://github.com/cloudfuse-io/ClickHouse/branches
[clickhouse_build_action]: https://github.com/cloudfuse-io/lambdatization/actions/workflows/helper-clickhouse.yaml
