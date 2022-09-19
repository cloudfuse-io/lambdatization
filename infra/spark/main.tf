variable "region_name" {}

variable "spark_image" {}

module "env" {
  source = "../common/env"
}

provider "aws" {
  region = var.region_name
  default_tags {
    tags = {
      module      = module.env.module_name
      provisioner = "terraform"
      stage       = terraform.workspace
    }
  }
}

module "spark_client" {
  source = "../common/lambda"

  function_base_name = "spark"
  region_name        = var.region_name
  docker_image       = var.spark_image
  memory_size        = 2048
  timeout            = 300

  additional_policies = []
  environment         = {}

}

output "lambda_name" {
  value = module.spark_client.lambda_name
}
