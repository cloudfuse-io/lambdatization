include "root" {
  path = find_in_parent_folders("terragrunt.${get_env("TF_STATE_BACKEND")}.hcl")
}

dependency "lambdacli" {
  config_path = "../lambdacli"

  mock_outputs = {
    lambda_name = "mock_name"
    lambda_arn  = "arn:aws:lambda:us-west-1:123456789012:function:mock_name"
  }
}

dependency "bigquery" {
  config_path = "../../monitoring/bigquery"
}

locals {
  region_name = get_env("L12N_AWS_REGION")
}

inputs = {
  region_name           = local.region_name
  lambdacli_lambda_name = dependency.lambdacli.outputs.lambda_name
  lambdacli_lambda_arn  = dependency.lambdacli.outputs.lambda_arn
}
