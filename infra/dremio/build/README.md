# Dremio lambdatization tricks

- Move the local path to `/tmp` as it is the only writeable one on lambda
- create a Dremio user and use its credentials to:
  - create a source
  - start the query
  - poll for the resulut
