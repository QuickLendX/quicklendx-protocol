import json
import sys

def parse_cargo_json(filepath):
    with open(filepath, 'r') as f:
        for line in f:
            try:
                data = json.loads(line)
                if 'message' in data and data.get('reason') == 'compiler-message':
                    msg = data['message']
                    if msg.get('level') == 'error':
                        print("ERROR:", msg.get('message'))
                        for span in msg.get('spans', []):
                            print(f"  --> {span.get('file_name')}:{span.get('line_start')}")
            except json.JSONDecodeError:
                pass

if __name__ == '__main__':
    parse_cargo_json(sys.argv[1])
