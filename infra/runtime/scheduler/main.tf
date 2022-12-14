variable "region_name" {}

variable "lambdacli_lambda_arn" {}

variable "lambdacli_lambda_name" {}

locals {
  standalone_engine_cmd   = "[ $(($RANDOM % 2)) = 0 ] || l12n init monitoring.bench-cold-warm"
  standalone_engine_input = "{\"cmd\":\"${base64encode(local.standalone_engine_cmd)}\"}"
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

resource "aws_cloudwatch_event_rule" "standalone_engine_schedule" {
  name                = "${module.env.module_name}-standalone-engine-sched-${module.env.stage}"
  description         = "Start standalone engine benchmark"
  schedule_expression = "rate(15 minutes)"
}

resource "aws_cloudwatch_event_target" "standalone_engine_schedule" {
  rule  = aws_cloudwatch_event_rule.standalone_engine_schedule.name
  arn   = var.lambdacli_lambda_arn
  input = local.standalone_engine_input
}

resource "aws_lambda_permission" "allow_cloudwatch" {
  action        = "lambda:InvokeFunction"
  function_name = var.lambdacli_lambda_name
  principal     = "events.amazonaws.com"
  source_arn    = aws_cloudwatch_event_rule.standalone_engine_schedule.arn
}
