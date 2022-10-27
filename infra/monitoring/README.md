# Monitoring Infra

To setup an instance of the monitoring infrastructure:
- create a GCP project and set it as L12N_GCP_PROJECT_ID
- choose a GCP region and set it as L12N_GCP_REGION
- before deploying, you must run `l12n monitoring.login` and authenticate
  yourself with an email that has access to the project ID specified above
- run `l12n monitoring.deploy`. Because of its different lifecycle, this
  deployment is independent of the rest of the infrastructure.
- check out the other `l12n monitoring.*` commands to discover how you can push
  metrics from the various execution setups.
