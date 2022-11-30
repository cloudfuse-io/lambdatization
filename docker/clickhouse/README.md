# ClickHouse lambdatization tricks

## :warning: Not working :warning:

Unfortunately, we didn't manage to run ClickHouse in AWS Lambda. It works
locally in the simulated environment, but on Lambda the server fails during
startup with the following trace:

```
2022.11.23 15:55:59.426316 [ 12 ] <Information> Application: Forked a child process to watch
2022.11.23 15:55:59.426531 [ 12 ] <Warning> Application: Cannot do prctl to ask termination with parent.
2022.11.23 15:55:59.426725 [ 12 ] <Information> SentryWriter: Sending crash reports is disabled
2022.11.23 15:55:59.426821 [ 10 ] <Information> Application: Will watch for the process with pid 12
2022.11.23 15:55:59.427574 [ 12 ] <Trace> Pipe: Pipe capacity is 1.00 MiB
2022.11.23 15:56:01.306313 [ 12 ] <Information> : Starting ClickHouse 22.10.2.2 (revision: 54467, git hash: f3cbbf9d3c34c6084259e23cb6ea1162e4f438d2, build id: 562C100ED804125D4844DBF8AC944E730CDDF34B), PID 12
2022.11.23 15:56:01.306500 [ 12 ] <Information> Application: starting up
2022.11.23 15:56:01.306519 [ 12 ] <Information> Application: OS name: Linux, version: 4.14.255-276-224.499.amzn2.x86_64, architecture: x86_64
2022.11.23 15:56:05.394083 [ 13 ] <Trace> BaseDaemon: Received signal 6
2022.11.23 15:56:05.394291 [ 14 ] <Fatal> BaseDaemon: ########################################
2022.11.23 15:56:05.394325 [ 14 ] <Fatal> BaseDaemon: (version 22.10.2.2 (official build), build id: 562C100ED804125D4844DBF8AC944E730CDDF34B) (from thread 12) (no query) Received signal Aborted (6)
2022.11.23 15:56:05.394355 [ 14 ] <Fatal> BaseDaemon: 
2022.11.23 15:56:05.394376 [ 14 ] <Fatal> BaseDaemon: Stack trace: 0x7faaf365d00b
2022.11.23 15:56:05.394416 [ 14 ] <Fatal> BaseDaemon: 0. gsignal @ 0x7faaf365d00b in ?
```

## List of tricks that we tried

- Clickhouse offers an Alpine version of its image, but we favor the Ubuntu one
  as we also used Debian/Ubuntu for the other images. The Alpine image is only
  30MB smaller so the gain wouldn't be huge anyway.
- Lambda does not support `prctl` with the `PR_SET_NAME` flag. We provide a
  [custom build](/.github/workflows/helper-clickhouse.yaml) that lazily fails
  when this command fails. To create a new image:
  - setup an [Ubuntu 20.04 runner](/.github/gh-runner-setup.sh)
  - create a branch in [cloudfuse-io/ClickHouse][cloudfuse_clickhouse_fork] with
    the patch
  - run the ClickHouse [build action][clickhouse_build_action]

[cloudfuse_clickhouse_fork]: https://github.com/cloudfuse-io/ClickHouse/branches
[clickhouse_build_action]: https://github.com/cloudfuse-io/lambdatization/actions/workflows/helper-clickhouse.yaml
