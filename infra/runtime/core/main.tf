variable "region_name" {
  description = "The AWS region name (eu-west-1, us-east2...) in which the stack will be deployed"
}

module "env" {
  source = "../../common/env"
}

data "aws_caller_identity" "current" {}


provider "aws" {
  region = var.region_name
  default_tags {
    tags = module.env.default_tags
  }
}

resource "aws_ecr_repository" "main" {
  name                 = "${module.env.module_name}-${module.env.stage}"
  image_tag_mutability = "MUTABLE"
  force_delete         = true

  image_scanning_configuration {
    scan_on_push = false
  }

  lifecycle {
    ignore_changes = [tags]
  }
}

resource "aws_s3_bucket" "data" {
  bucket = "${module.env.module_name}-${data.aws_caller_identity.current.account_id}-${var.region_name}-${module.env.stage}"
}

resource "aws_s3_object_copy" "nyc_taxi" {
  count  = 2
  bucket = aws_s3_bucket.data.id
  key    = "nyc-taxi/2019/0${count.index + 1}/data.parquet"
  source = "ursa-labs-taxi-data/2019/0${count.index + 1}/data.parquet"

  lifecycle {
    ignore_changes = [tags_all]
  }
}

output "repository_url" {
  value = aws_ecr_repository.main.repository_url
}

output "repository_arn" {
  value = aws_ecr_repository.main.arn
}

output "bucket_name" {
  value = aws_s3_bucket.data.id
}

output "bucket_arn" {
  value = aws_s3_bucket.data.arn
}
