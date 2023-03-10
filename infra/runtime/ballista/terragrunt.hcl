include "root" {
  path = find_in_parent_folders("terragrunt.${get_env("TF_STATE_BACKEND")}.hcl")
}

dependency "core" {
  config_path = "../core"

  mock_outputs = {
    bucket_arn = "arn:aws:s3:::mock"
  }
}

locals {
  region_name = get_env("L12N_AWS_REGION")
}


terraform {
  before_hook "deploy_images" {
    commands = ["apply"]
    execute  = ["../build_and_print.sh", "ballista"]
  }

  extra_arguments "image_vars" {
    commands  = ["apply"]
    arguments = ["-var-file=${get_terragrunt_dir()}/images.generated.tfvars"]
  }

}

inputs = {
  region_name                = local.region_name
  ballista_image             = "dummy_overriden_by_before_hook"
  ballista_distributed_image = "dummy_overriden_by_before_hook"
  bucket_arn                 = dependency.core.outputs.bucket_arn
}
