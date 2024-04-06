#!/usr/bin/env python3

import argparse
import os

import yaml

verbose = False
log_prefix = '[Phantom SNAP]'
DEBUG = False
arch_table = {'arm64': 'aarch64-linux-gnu',
              'amd64': 'x86_64-linux-gnu'}
missing_vars = []


def pa_print(message):
    print(f'{log_prefix} {message}', flush=True)


def pa_print_debug(message):
    if DEBUG is True:
        pa_print(message)


def main():
    parser = argparse.ArgumentParser(
        description='Replace placeholders in input file.')
    parser.add_argument('inputfile', action='store', type=str, default='templates/snapcraft_template.yaml',
                        metavar='YAMLFILE', nargs='?', help='YAML file to parse')
    parser.add_argument('outputfile', action='store', type=str, default='snap/snapcraft.yaml',
                        metavar='OUTPUTFILE', nargs='?',
                        help='Optional output file. If omitted, the snap/snapcraft.yaml is overwritten')
    parser.add_argument('--version', action='store', type=str, metavar='APP_VERSION', nargs='?', default='0.2',
                        help='The snap version. If omitted, will use a default value')
    parser.add_argument('--arch', action='store', type=str, metavar='ARCH', nargs='?', default='arm64',
                        help='The target architecture. If omitted, will use the default value')
    parser.add_argument('-v', action='store_true', help='Verbose')
    args = parser.parse_args()
    infile = args.inputfile
    outfile = args.outputfile
    # version in snap yaml is limited to 32 chars, snap will fail build if longer
    version = args.version
    arch = args.arch
    global verbose
    verbose = args.v

    if verbose:
        pa_print(f'Processing placeholders in {infile}')

    with open(infile, 'r') as instream:
        y = yaml.safe_load(instream)

        # replace placeholders
        y['version'] = version

    outfile_dir = os.path.dirname(os.path.abspath(outfile))
    # check dir exists
    if not os.access(outfile_dir, os.F_OK | os.W_OK):
        pa_print(f'Could not access {outfile_dir}')
        exit(1)

    if verbose:
        pa_print(f' * Writing changes to {outfile}')

    with open(outfile, 'w') as outstream:
        yaml.dump(y, outstream, default_flow_style=False, sort_keys=False)


if __name__ == "__main__":
    main()
