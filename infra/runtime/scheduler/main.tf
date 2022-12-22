variable "region_name" {}

variable "lambdacli_lambda_arn" {}

variable "lambdacli_lambda_name" {}

module "env" {
  source = "../../common/env"
}

provider "aws" {
  region = var.region_name
  default_tags {
    tags = module.env.default_tags
  }
}

locals {
  # randomly cancel the benchmarks to make them less predictable
  randomize = "[ $(($RANDOM % 2)) = 0 ] ||"
}

## STANDALONE ENGINES

locals {
  standalone_engine_cmd   = "${local.randomize} l12n init monitoring.bench-cold-warm"
  standalone_engine_input = "{\"cmd\":\"${base64encode(local.standalone_engine_cmd)}\"}"
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

resource "aws_lambda_permission" "allow_standalone_engine" {
  action        = "lambda:InvokeFunction"
  function_name = var.lambdacli_lambda_name
  principal     = "events.amazonaws.com"
  source_arn    = aws_cloudwatch_event_rule.standalone_engine_schedule.arn
}

## LAMBDA SCALE UP

locals {
  scales         = [50, 100, 200]
  scaling_cmds   = [for n in local.scales : "${local.randomize} l12n init monitoring.bench-scaling -n ${n}"]
  scaling_inputs = [for s in local.scaling_cmds : "{\"cmd\":\"${base64encode(s)}\"}"]
}

resource "aws_cloudwatch_event_rule" "scaling_schedule" {
  count               = length(local.scales)
  name                = "${module.env.module_name}-scaling-sched-${local.scales[count.index]}-${module.env.stage}"
  description         = "Start scaling benchmark with ${local.scales[count.index]} functions"
  schedule_expression = "cron(${count.index * 10 + 4} * * * ? *)"
}

resource "aws_cloudwatch_event_target" "scaling_schedule" {
  count = length(local.scales)
  rule  = aws_cloudwatch_event_rule.scaling_schedule[count.index].name
  arn   = var.lambdacli_lambda_arn
  input = local.scaling_inputs[count.index]
}

resource "aws_lambda_permission" "allow_scaling" {
  count         = length(local.scales)
  action        = "lambda:InvokeFunction"
  function_name = var.lambdacli_lambda_name
  principal     = "events.amazonaws.com"
  source_arn    = aws_cloudwatch_event_rule.scaling_schedule[count.index].arn
}
