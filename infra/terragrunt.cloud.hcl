generate "backend" {
  path      = "backend.generated.tf"
  if_exists = "overwrite"
  contents  = <<EOC
terraform {
  cloud {
    organization = "${get_env("TF_ORGANIZATION")}"
    token        = "${get_env("TF_API_TOKEN")}"
    workspaces {
      name = "${get_env("TF_WORKSPACE_PREFIX")}${replace(path_relative_to_include(), "/", "-")}"
    }
  }
}
EOC
}


locals {
  common = read_terragrunt_config(find_in_parent_folders("common.hcl"))
}

terraform {
  extra_arguments "data_dir" {
    commands = local.common.locals.extra_arguments.commands
    env_vars = {
      TF_DATA_DIR = "${local.common.locals.extra_arguments.data_dir}/${path_relative_to_include()}"
      TF_PLUGIN_CACHE_DIR = "${local.common.locals.extra_arguments.data_dir}"
    }
  }
}

generate = local.common.generate
