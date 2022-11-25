# Tips about query engine metrics in AWS Lambda

- All engines are deployed in "single node" mode.
- They perform an aggregation on a Parquet file (cold start), then are invoked
  again right away on a different Parquet file (warm start). Both files are NYC
  Taxi Parquet archives of approximatively 120MB.
- The aggregation is a GROUP BY on the "payment type" column and a SUM on the
  "trip distance" column. This query is at the same time simple enough for all
  engines to easily support it but complex enough to force the engine to scan at
  least 2 full columns from the Parquet file.
- The durations displayed cover the entire invocation time of the lambda
  function as perceived by the client.
