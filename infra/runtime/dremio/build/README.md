# Dremio lambdatization tricks

- Move the local path to `/tmp` as it is the only writeable one on lambda
- create a Dremio user and use its credentials to:
  - create a source
  - start the query
  - poll for the result
- By default Dremio tries to discover its private IP and uses that to
  communicate. We want to loopback on `localhost` instead, hence the
  configuration `registration.publish-host: "localhost"`
