variable "region_name" {}

variable "chappydev_image" {}

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
  name = "${module.env.module_name}-chappydev-s3-access-${var.region_name}-${module.env.stage}"

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

resource "aws_iam_policy" "lambda_insights" {
  name = "${module.env.module_name}-chappydev-lambda-insights-${var.region_name}-${module.env.stage}"

  policy = <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": "logs:CreateLogGroup",
            "Resource": "*"
        },
        {
            "Effect": "Allow",
            "Action": [
                "logs:CreateLogStream",
                "logs:PutLogEvents"
            ],
            "Resource": "arn:aws:logs:*:*:log-group:/aws/lambda-insights:*"
        }
    ]
}
EOF
}

module "dev_lambda" {
  source = "../../common/lambda"

  function_base_name = "chappydev"
  region_name        = var.region_name
  docker_image       = var.chappydev_image
  memory_size        = 2048
  timeout            = 300

  additional_policies = [aws_iam_policy.s3_access.arn, aws_iam_policy.lambda_insights.arn]
  environment         = {}

}

output "dev_lambda_name" {
  value = module.dev_lambda.lambda_name
}
