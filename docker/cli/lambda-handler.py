import logging


logging.getLogger().setLevel(logging.INFO)


def handler(event, context):
    """AWS Lambda handler"""
    logging.info("hello from Lambda CLI")
    return {"result": "cli"}
