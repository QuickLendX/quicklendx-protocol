import os
import glob

replacements = {
    '—': '-',
    '─': '-',
    '→': '->',
    '–': '-',
    '“': '"',
    '”': '"',
    '’': "'",
    '‘': "'"
}

for filepath in glob.glob('/mnt/c/Users/fuhad/quicklendx-protocol/quicklendx-contracts/src/**/*.rs', recursive=True):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    modified = False
    for k, v in replacements.items():
        if k in content:
            content = content.replace(k, v)
            modified = True
            
    new_content = ""
    for char in content:
        if ord(char) > 127:
            new_content += '-'
            modified = True
        else:
            new_content += char

    if modified:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(new_content)
        print(f"Cleaned {filepath}")
