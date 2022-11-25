# Tips about AWS Lambda scaling metrics

- The durations displayed represent the end to end execution time to complete
  the invocation of a given number of AWS Lambda functions in parallel.
- The percentiles displayed represent the distribution of the batch execution
  durations and not the distribution of invocation duration within batches.
- To avoid re-using existing Lambda containers (warm starts) during the
  invocation of a batch, and thus properly measure parallelism, we maintain the
  functions running during a given "sleep duration". This duration is then
  discounted from the batch invocation duration.​
- We run different images, all using the same lightweight base image (~40MB),
  but embed a differently sized "payload file".
