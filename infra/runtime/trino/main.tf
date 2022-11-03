variable "region_name" {}

variable "trino_image" {}

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
  name = "${module.env.module_name}-trino-s3-access-${var.region_name}-${module.env.stage}"

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

  function_base_name = "trino"
  region_name        = var.region_name
  docker_image       = var.trino_image
  memory_size        = 2048
  timeout            = 300

  additional_policies = [aws_iam_policy.s3_access.arn]
  environment         = {}

}

output "lambda_name" {
  value = module.engine.lambda_name
}
