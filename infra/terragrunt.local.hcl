remote_state {
  backend = "local"
  generate = {
    path      = "backend.generated.tf"
    if_exists = "overwrite"
  }
  config = {
    path = "${get_env("CALLING_DIR")}/.terraform/state/${path_relative_to_include()}/terraform.tfstate"
  }
}

locals {
  common = read_terragrunt_config(find_in_parent_folders("common.hcl"))
}

terraform {
  extra_arguments "data_dir" {
    commands = local.common.locals.extra_arguments.commands
    env_vars = {
      TF_DATA_DIR         = "${local.common.locals.extra_arguments.data_dir}/${path_relative_to_include()}"
    }
  }
}

generate = local.common.generate
