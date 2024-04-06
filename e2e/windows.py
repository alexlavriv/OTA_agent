import unittest

from e2e import tools
from e2e.tools import *


class WindowsBase(unittest.TestCase):
    def test_something_special(self):
        self.assertEqual(True, False)  # add assertion here


import os.path
from os import path
import requests
import os
import time
import psutil

PHANTOM_DOWNLOAD_PATH = r'C:\Program Files\phantom_agent\bin\download'
AUTOMATION_FILE = PHANTOM_DOWNLOAD_PATH + r'\automation_file.txt'
PHANTOM_AGENT_URI = 'http://localhost:30000/update_version'
PHANTOM_AGENT_MANIFEST_PATH = r'C:\Program Files\phantom_agent\bin\hash_manifest'


def init():
    print("Init")
    tools.create_file(AUTOMATION_FILE)
    if tools.file_exist(AUTOMATION_FILE):
        print("Created automation file succefully")
    print(f"Erasing {PHANTOM_AGENT_MANIFEST_PATH}")
    erase_file(PHANTOM_AGENT_MANIFEST_PATH)


def call_update_version():
    perform_rest_call(PHANTOM_AGENT_URI)


def erase_file(path):
    if os.path.exists(path):
        os.remove(path)
    else:
        print(f"The file {path} does not exist")


def perform_rest_call(uri):
    response = requests.get(uri)
    response_code = response.status_code
    print(f"Called {uri}, response code is: {response_code}")
    return response_code == 200


def run():
    iteration_number = 1
    while (True):
        print(f'Starting iteration number {iteration_number}')
        start = time.time()
        run_once()
        m, s = divmod(time.time() - start, 60)
        print(f'Time took {m} minutes {s} seconds')
        print(f'Done iteration number {iteration_number}')
        iteration_number = iteration_number + 1


def is_service_running(name):
    service = None
    try:
        service = psutil.win_service_get(name)
        service = service.as_dict()
    except Exception as ex:
        print(str(ex))
    if service and service['status'] == 'running':
        print(f"{name} service is running")
        return True
    else:
        print(f"{name} service is not running")
        return False


def run_once():
    init()
    call_update_version()
    while file_exist(AUTOMATION_FILE):
        if not is_service_running('PhantomAgent'):
            print("PhantomAgent Service crashed")
        print("Phantom agent is running an update, going to sleep")
        time.sleep(5)
    print("Update ended")


def main():
    print("Start")
    run()


if __name__ == "__main__":
    main()
