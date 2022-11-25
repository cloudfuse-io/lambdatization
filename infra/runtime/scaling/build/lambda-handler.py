import os
import logging
import time

logging.getLogger().setLevel(logging.INFO)

IS_COLD_START = True


def handler(event, context):
    global IS_COLD_START
    is_cold_start = IS_COLD_START
    IS_COLD_START = False
    placeholder_size = os.path.getsize("/placeholder.bin")
    # We can set a sleep duration to make sure every invocation is alocated to a
    # new Lambda container and doesn't trigger a warm start. Sleeping on warm
    # start would bias the corrected duration (observed duration - sleep time)
    if is_cold_start:
        time.sleep(event.get("sleep", 0))
    return {
        "placeholder_size": placeholder_size,
        "cold_start": is_cold_start,
        "memory_limit_in_mb": int(context.memory_limit_in_mb),
    }


if __name__ == "__main__":
    res = handler({}, {})
    print(res)
