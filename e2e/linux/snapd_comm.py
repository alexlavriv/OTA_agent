import requests
import socket
import pprint

from urllib3.connection import HTTPConnection
from urllib3.connectionpool import HTTPConnectionPool
from requests.adapters import HTTPAdapter

import time

import warnings


class SnapInfo:
    def __init__(self, snap_name, is_daemon, is_running, install_counter):
        self.snap_name = snap_name
        self.is_daemon = is_daemon
        self.is_running = is_running
        self.install_counter = install_counter


def revision_to_int(revision):
    revision = revision[1:]
    return int(revision)


def get_snap_info(snap_name):
    session = requests.Session()
    session.mount("http://snapd/", SnapdAdapter())
    response = session.get("http://snapd/v2/snaps")
    warnings.filterwarnings(action="ignore", message="unclosed", category=ResourceWarning)
    response_json = response.json()
    for entry in response_json['result']:
        if entry["name"] == snap_name:
            revision = revision_to_int(entry["revision"])
            is_daemon = False
            is_running = False
            daemon_entry_list = [app for app in entry["apps"] if "daemon" in app]
            if len(daemon_entry_list) == 1:
                is_daemon = True
                is_running = "active" in daemon_entry_list[0]
                is_running = is_running and daemon_entry_list[0]['active']
            break

    return SnapInfo(snap_name, is_daemon, is_running, revision)


def is_snap_installed(snap_name):
    session = requests.Session()
    session.mount("http://snapd/", SnapdAdapter())
    response = session.get("http://snapd/v2/snaps")
    warnings.filterwarnings(action="ignore", message="unclosed", category=ResourceWarning)
    response_json = response.json()
    for entry in response_json['result']:
        if entry["name"] == snap_name:
            print(entry["version"])
            return True

    return False


def get_snap_install_counter(snap_name):
    session = requests.Session()
    session.mount("http://snapd/", SnapdAdapter())
    response = session.get("http://snapd/v2/snaps")
    warnings.filterwarnings(action="ignore", message="unclosed", category=ResourceWarning)
    response_json = response.json()
    for entry in response_json['result']:
        if entry["name"] == snap_name:
            return entry["revision"]

    return -1


def get_snap_version(snap_name):
    return NotImplemented


class SnapdConnection(HTTPConnection):
    def __init__(self):
        super().__init__("localhost")

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect("/run/snapd.socket")


class SnapdConnectionPool(HTTPConnectionPool):
    def __init__(self):
        super().__init__("localhost")

    def _new_conn(self):
        return SnapdConnection()


class SnapdAdapter(HTTPAdapter):
    def get_connection(self, url, proxies=None):
        return SnapdConnectionPool()

# def main():
#     session = requests.Session()
#     session.mount("http://snapd/", SnapdAdapter())
#     response = session.get("http://snapd/v2/system-info")
#     pprint.pprint(response.json())
#     is_snap_installed("phantom-agent")
#
#
# if __name__ == '__main__':
#     main()
