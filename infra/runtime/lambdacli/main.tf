variable "region_name" {}

variable "lambdacli_image" {}

variable "bucket_arn" {}

variable "env_file" {
  sensitive = true
}

module "env" {
  source = "../../common/env"
}

provider "aws" {
  region = var.region_name
  default_tags {
    tags = module.env.default_tags
  }
}

resource "aws_secretsmanager_secret" "envfile" {
  name = "${module.env.module_name}-clienv-${module.env.stage}"
}

resource "aws_secretsmanager_secret_version" "envfile" {
  secret_id     = aws_secretsmanager_secret.envfile.id
  secret_string = var.env_file
}

resource "aws_iam_policy" "secret_access" {
  name = "${module.env.module_name}-lambdacli-secret-${var.region_name}-${module.env.stage}"

  policy = <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": "secretsmanager:GetSecretValue",
      "Resource": "${aws_secretsmanager_secret.envfile.arn}"
    }
  ]
}
EOF
}

module "lambdacli" {
  source = "../../common/lambda"

  function_base_name = "lambdacli"
  region_name        = var.region_name
  docker_image       = var.lambdacli_image
  memory_size        = 2048
  ephemeral_storage  = 2048
  timeout            = 300

  additional_policies = [aws_iam_policy.secret_access.arn]

  environment = {
    ENV_FILE_SECRET_ID         = aws_secretsmanager_secret.envfile.id
    ENV_FILE_SECRET_VERSION_ID = aws_secretsmanager_secret_version.envfile.version_id
    # region is required early to get the secret
    L12N_AWS_REGION = var.region_name
  }
}

output "lambda_name" {
  value = module.lambdacli.lambda_name
}
