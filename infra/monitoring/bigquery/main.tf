variable "region_name" {}
variable "project_id" {}

module "env" {
  source = "../../common/env"
}

provider "google" {
  project = var.project_id
  region  = var.region_name
}

resource "google_service_account" "bigquery_write" {
  account_id   = "${module.env.module_name}-bigquery-write-${module.env.stage}"
  display_name = "BigQuery Write"
}

resource "google_project_iam_member" "firestore_owner_binding" {
  role   = "roles/bigquery.dataEditor"
  member = "serviceAccount:${google_service_account.bigquery_write.email}"
}

resource "google_service_account_key" "bigquery_write_key" {
  service_account_id = google_service_account.bigquery_write.name
}

resource "google_bigquery_dataset" "dataset" {
  dataset_id                  = "${module.env.module_name}_metrics_${module.env.stage}"
  friendly_name               = "Lambdatization Metrics"
  default_table_expiration_ms = 1576800000000 # 50 years ;-)
  labels                      = module.env.default_tags
}

resource "google_bigquery_table" "standalone_engine_durations" {
  dataset_id = google_bigquery_dataset.dataset.dataset_id
  table_id   = "${module.env.module_name}-standalone-engine-durations-${module.env.stage}"
  labels     = module.env.default_tags

  time_partitioning {
    type  = "DAY"
    field = "timestamp"
  }

  schema = <<EOF
[
  {
    "name": "engine",
    "mode": "NULLABLE",
    "type": "STRING"
  },
  {
    "name": "initduration",
    "mode": "NULLABLE",
    "type": "INTEGER"
  },
  {
    "name": "external_duration_ms",
    "mode": "NULLABLE",
    "type": "INTEGER"
  },
  {
    "name": "cold_start",
    "mode": "NULLABLE",
    "type": "BOOLEAN"
  },
  {
    "name": "timestamp",
    "mode": "NULLABLE",
    "type": "TIMESTAMP"
  }
]
EOF
}

output "service_account_key" {
  value     = google_service_account_key.bigquery_write_key.private_key
  sensitive = true
}
