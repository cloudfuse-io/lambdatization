#!/bin/bash

set -e

send="l12n send-metrics --gcp-creds-file=/host/home/ubuntu/.config/gcloud/bigquery-push.json --bigquery-table-id=gcp-dashboard-365510.lambdatization.exec_durations"


while [ true ]
do
    l12n databend.lambda-example -j | $send
    l12n spark.lambda-example-hive -j | $send
    l12n dremio.lambda-example -j | $send
    l12n databend.lambda-example -j | $send
    l12n spark.lambda-example-hive -j | $send
    l12n dremio.lambda-example -j | $send
    sleep 3600
done
