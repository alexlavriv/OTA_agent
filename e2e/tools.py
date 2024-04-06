import os.path
from os import path
import subprocess


def create_file(file_path):
    print(f"Creating {file_path}")
    f = open(file_path, "a")
    f.write("Automation file")
    f.close()


def file_exist(file_path):
    return path.exists(file_path)


def execute_command(command_arr):
    subprocess.run(command_arr)
