variable "region_name" {}

variable "lambdacli_image" {}

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

module "scheduler" {
  source = "../../common/lambda"

  function_base_name = "scheduler"
  region_name        = var.region_name
  docker_image       = var.lambdacli_image
  memory_size        = 2048
  timeout            = 300

  additional_policies = []
  environment         = {}
}

output "lambda_name" {
  value = module.scheduler.lambda_name
}
