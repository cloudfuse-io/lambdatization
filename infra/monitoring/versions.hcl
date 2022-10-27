generate "versions" {
  path      = "versions.generated.tf"
  if_exists = "overwrite"
  contents  = <<EOF
terraform {
  required_version = ">=1"
  required_providers {
    google = {
      source = "hashicorp/google"
      version = "~> 3.0"
    }
  }
}
EOF
}
