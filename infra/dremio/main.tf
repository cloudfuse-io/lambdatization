variable "region_name" {}

variable "dremio_image" {}

module "env" {
  source = "../common/env"
}

provider "aws" {
  region = var.region_name
  default_tags {
    tags = module.env.default_tags
  }
}

# The Java SDK always needs credentials to operate, so it needs access to S3 even if the bucket is public
resource "aws_iam_policy" "s3_access" {
  name = "${module.env.module_name}-dremio-s3-access-${var.region_name}-${module.env.stage}"

  policy = <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Sid": "listbuckets",
            "Effect": "Allow",
            "Action": [
                "s3:GetBucketLocation",
                "s3:ListAllMyBuckets"
            ],
            "Resource": [
                "arn:aws:s3:::*"
            ]
        },
        {
            "Sid": "objectlevel",
            "Effect": "Allow",
            "Action": "s3:*",
            "Resource": "arn:aws:s3:::ursa-labs-taxi-data/*"
        },
        {
            "Sid": "bucketlevel",
            "Effect": "Allow",
            "Action": "s3:*",
            "Resource": "arn:aws:s3:::ursa-labs-taxi-data"
        }
    ]
}
EOF
}

module "engine" {
  source = "../common/lambda"

  function_base_name = "dremio"
  region_name        = var.region_name
  docker_image       = var.dremio_image
  memory_size        = 2048
  timeout            = 300

  additional_policies = [aws_iam_policy.s3_access.arn]
  environment         = {}

}

output "lambda_name" {
  value = module.engine.lambda_name
}
