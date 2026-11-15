import re
import time


SETTLING_SECONDS = 5


def total_pss_kib(text):
    values = re.findall(r"^[ \t]*TOTAL(?: PSS)?:[ \t]*(\d+)(?=[ \t]|$)", text, re.MULTILINE)
    if len(values) != 1:
        raise ValueError("meminfo did not report one unambiguous total PSS")
    return int(values[0])


def resident_memory_kib(text):
    result = {}
    for field, key in (("VmRSS", "rss_kib"), ("VmHWM", "reported_rss_hwm_kib")):
        lines = [line for line in text.splitlines() if line.startswith(field + ":")]
        value = re.fullmatch(field + r":[ \t]*(\d+)[ \t]+kB[ \t]*", lines[0]) if len(lines) == 1 else None
        if value is None:
            raise ValueError("process status did not report one unambiguous " + field + " in kB")
        result[key] = int(value[1])
    return result


def process_stat(text):
    fields = text.rpartition(") ")[2].split()
    if len(fields) < 20:
        raise ValueError("incomplete process stat")
    return {"start_ticks": int(fields[19]), "cpu_ticks": int(fields[11]) + int(fields[12])}


def resident_observation(read, expected_start_ticks):
    before = process_stat(read("stat"))
    memory = resident_memory_kib(read("status"))
    after = process_stat(read("stat"))
    if before["start_ticks"] != expected_start_ticks or after["start_ticks"] != expected_start_ticks:
        raise ValueError("process identity changed during resident-memory observation")
    return {**memory, "start_ticks": expected_start_ticks}


def cpu_observation(read, sleep, expected_start_ticks, seconds):
    before = read()
    started = time.monotonic()
    sleep(seconds)
    after = read()
    if before["start_ticks"] != expected_start_ticks or after["start_ticks"] != expected_start_ticks or after["cpu_ticks"] < before["cpu_ticks"]:
        raise ValueError("process identity or CPU counter changed during observation")
    return {"seconds": time.monotonic() - started, "requested_seconds": seconds, "cpu_ticks": after["cpu_ticks"] - before["cpu_ticks"]}
