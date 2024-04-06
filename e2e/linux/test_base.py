import unittest

from .snapd_comm import *


PHANTOM_AGENT_URI = 'http://localhost:30000/update_version_force'
SNAP_SERVICE_NAME = 'phantom-agent'
PHANTOM_DOWNLOAD_PATH = r'/root/snap/phantom-agent/common'
AUTOMATION_FILE = r'/root/snap/phantom-agent/common/hash_manifest'

def revision_to_int(revision):
    revision = revision[1:]
    return int(revision)

def perform_rest_call(uri):
    response = requests.get(uri)
    response_code = response.status_code
    print(f"Called {uri}, response code is: {response_code}")
    return response_code == 200

def init():
    print("Init")
    tools.create_file(AUTOMATION_FILE)
    if tools.file_exist(AUTOMATION_FILE):
        print("Created automation file succefully")

class LinuxBase(unittest.TestCase):
    def __init__(self, *args, **kwargs):
        super(LinuxBase, self).__init__(*args, **kwargs)
        print("LinuxBase")

    def test_is_agent_snap_installed(self):
        actual = is_snap_installed(SNAP_SERVICE_NAME)
        self.assertEqual(True, actual)

    def test_update_version_force(self):
        #init()
        snap_info = get_snap_info(SNAP_SERVICE_NAME)
        before_update_counter = snap_info.install_counter
        after_update_counter = before_update_counter
        perform_rest_call(PHANTOM_AGENT_URI)
        while before_update_counter == snap_info.install_counter:
            print("running test, going to sleep")
            time.sleep(5)
            snap_info = get_snap_info(SNAP_SERVICE_NAME)


        self.assertEqual(before_update_counter + 1, snap_info.install_counter)
        self.assertTrue(snap_info.is_running)


if __name__ == '__main__':
    unittest.main()


