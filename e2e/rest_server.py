# app.py
from flask import Flask, request, jsonify, send_from_directory, Response
import pathlib

app = Flask(__name__)

@app.get("/api/v1/requestChallenge")
def get_countries():
    print("requestChallenge called")
    challange = jsonify({"challenge": "yAMDdBSWKirB3/wtNsmLVAfIZ2dx3OF6OyYda6n6k1M="})
    return challange


@app.post("/api/v1/validateChallenge")
def add_country():
    response = {
        "verified": True,
        "license": {
            "id": "60472f20aa32d50018690dd9",
            "label": "PHANTOM-IL-DESKTOP-ALEX"
        },
        "node": {
            "id": "60472f20aa32d50018690dd9",
            "label": "PHANTOM-IL-DESKTOP-ALEX"
        },
        "token": "eyJhbGciOiJQUzI1NiIsImtpZCI6InpZMEM5WVlLcmNiMS0tdEloNERYXyJ9.eyJpc3MiOiJ0c3QucGhhbnRvbWF1dG8uY29tLyIsImF1ZCI6InRzdC5waGFudG9tYXV0by5jb20vY291cGxpbmciLCJzdWIiOiI2MDQ3MmYyMGFhMzJkNTAwMTg2OTBkZDkiLCJleHAiOjE2Mzk2Njc2MTAsImlhdCI6MTYzOTU4MTIxMH0.W2w1OaUZtZSbV8zG6Y7ubCCWTJXzWc5759X1iLlUWV6SxakpLnVLeWzFfhAm3DkGJzY-SFOZhZ4hr2YO-tCd4X_oDz2HTNrDECh1AsMJ3rKXBAOzbgaYaUDdz88FIKmJAdHTuU2IZG-IessGbtfSZdy6a7ckBdnExCii3SwcftWnWqTQiBTT5VNXSyYbmXLmsQxV8nB1Toil84sZrthtQKFppPYfk1wZ6o03cFyMmF2TUbhIMQ-SsL-bz_9fhpW5j_DGdkuVPzJStslfpk0FxAoizVOLOms0YovL6TJaB2bHFla8vmIdYs9PLbaFrIrrGf-0NfbHVW_c5j9_2GIXYA"
    }

    return jsonify(response)


def create_manifest(checksums):
    database = {
        "core": {
            "token": "",
            "_id": "621c78f3fd12780012c795da",
            "component": "core",
            "version": "3.0.25",
            "link": "http://localhost:5000/core.snap",
            "checksum": "f1f83270cca5d441dd9dbf0e3a30b197317038c0",
            "arch": "AMD64"
        },
        "phantom_agent": {
            "token": "",
            "_id": "621c78f3fd12780012c795da",
            "component": "phantom_agent",
            "version": "0.6.14",
            "link": "http://localhost:5000/agent.snap",
            "checksum": "5775bda8db07aa301b2360b17576e0409a649fc4",
            "arch": "AMD64"
        }
    }
    manifest = []
    for key, value in checksums.items():
        if key in database and database.get(key).get('checksum') != value:
            manifest.append(database.get(key))
    return manifest

@app.post("/api/v1/versions/manifest")
def post_manifest():
    request_data = request.get_json()
    print(request_data)
    response = create_manifest(request_data.get("checksums"))
    return jsonify(response)


@app.get("/core.snap")
def get_core_snap():
    current_path = pathlib.Path(__file__).parent.resolve()
    print(f"Current path is {current_path}")
    response = send_from_directory(directory='./artifacts', path='phau-core.snap')
    return response


@app.get("/core.snap/api/storage_test")
def get_core_size():
    response = jsonify({"size": "314572800"})
    return response

@app.get("/agent.snap")
def get_agent_snap():
    response = send_from_directory(directory='./artifacts', path='phantom-agent.snap')
    return response


@app.get("/agent.snap/api/storage_test")
def get_agent_size():
    response = jsonify({"size": "314572800"})
    return response


@app.put("/api/v1/nodes/self/ota-status")
def put_ota_status():
    request_data = request.get_json()
    print(request_data)
    return Response(status=200, mimetype='application/json')

if __name__ == "__main__":
    app.run()
