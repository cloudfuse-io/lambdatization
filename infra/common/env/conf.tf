locals {
  module_name = "l12n"
}

# use the workspace name as stage
output "stage" {
  value = terraform.workspace
}

output "module_name" {
  value = local.module_name
}


output "default_tags" {
  value = {
      module      = local.module_name
      provisioner = "terraform"
      stage       = terraform.workspace
    }
}
