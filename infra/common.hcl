generate "versions" {
  path      = "versions.generated.tf"
  if_exists = "overwrite"
  contents  = <<EOF
terraform {
  required_version = ">=1"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 4.45"
    }
    google = {
      source = "hashicorp/google"
      version = "~> 3.0"
    }
  }
}
EOF
}

locals {
  extra_arguments = {
    commands = [
      "init",
      "apply",
      "destroy",
      "output",
      "fmt",
    ]

    data_dir = "${get_env("CALLING_DIR")}/.terraform/data/"
  }
}
