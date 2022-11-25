variable "region_name" {}

variable "images" {
  type = list(string)
}

variable "placeholder_sizes" {
  type = list(number)
}

locals {
  # tflint-ignore: terraform_unused_declarations
  validate_sizes = (length(var.placeholder_sizes) != length(var.images)) ? tobool("Placeholder size list and image name list must have the same size.") : true
}

variable "bucket_arn" {}

module "env" {
  source = "../../common/env"
}

provider "aws" {
  region = var.region_name
  default_tags {
    tags = module.env.default_tags
  }
}

module "placeholder" {
  source = "../../common/lambda"
  count  = length(var.placeholder_sizes)

  function_base_name = "placeholder-${element(var.placeholder_sizes, count.index)}"
  region_name        = var.region_name
  docker_image       = element(var.images, count.index)
  memory_size        = 2048
  timeout            = 300

  additional_policies = []
  environment         = {}
}

output "lambda_names" {
  value = join(",", module.placeholder.*.lambda_name)
}
