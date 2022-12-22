# Tips about AWS Lambda scaling metrics

- The total duration represents the end to end execution time to complete
  the invocation of a given number of AWS Lambda functions in parallel.
- The percentiles `PX` represent the time to successfuly complete `X` % of the
  function runs.
- To avoid re-using existing Lambda containers (warm starts) during the
  invocation of a batch, and thus properly measure parallelism, we maintain the
  functions running during a given "sleep duration". This duration is then
  discounted from the batch invocation duration.​
- We run different images, all using the same lightweight base image (~40MB),
  but embed a differently sized "payload file".
