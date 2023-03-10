variable "region_name" {}

variable "ballista_standalone_image" {}

variable "ballista_distributed_image" {}

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

resource "aws_iam_policy" "s3_access" {
  name = "${module.env.module_name}-ballista-s3-access-${var.region_name}-${module.env.stage}"

  policy = <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Sid": "objectlevel",
            "Effect": "Allow",
            "Action": "s3:*",
            "Resource": "${var.bucket_arn}/*"
        },
        {
            "Sid": "bucketlevel",
            "Effect": "Allow",
            "Action": "s3:*",
            "Resource": "${var.bucket_arn}"
        }
    ]
}
EOF
}

module "engine" {
  source = "../../common/lambda"

  function_base_name = "ballista"
  region_name        = var.region_name
  docker_image       = var.ballista_standalone_image
  memory_size        = 2048
  timeout            = 300

  additional_policies = [aws_iam_policy.s3_access.arn]
  environment         = {}

}

module "distributed_engine" {
  source = "../../common/lambda"

  function_base_name = "ballista-distributed"
  region_name        = var.region_name
  docker_image       = var.ballista_distributed_image
  memory_size        = 2048
  timeout            = 300

  additional_policies = [aws_iam_policy.s3_access.arn]
  environment = {
    CHAPPY_VIRTUAL_SUBNET : "172.28.0.0/16",
  }

}

output "lambda_name" {
  value = module.engine.lambda_name
}

output "distributed_lambda_name" {
  value = module.distributed_engine.lambda_name
}
