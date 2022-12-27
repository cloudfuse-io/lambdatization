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

## STANDALONE ENGINES

locals {
  standalone_engine_cmd   = "[ $(($RANDOM % 2)) = 0 ] || l12n init monitoring.bench-cold-warm"
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
  scales = [64, 128, 256]
  # run larger tests less often
  scaling_cmds   = [for sc in local.scales : "[ $(($RANDOM % ${sc / 32})) = 0 ] l12n init monitoring.bench-scaling -n ${sc}"]
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
