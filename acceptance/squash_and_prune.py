import subprocess

def run(cmd):
    print("Running:", cmd)
    p = subprocess.run(cmd, shell=True, cwd=r"D:\projects\ruby-agent", capture_output=True, text=True)
    if p.stdout:
        print(p.stdout.strip())
    if p.stderr:
        print(p.stderr.strip())

run("git checkout --orphan clean_main")
run("git add -A")
run('git commit -m "Initial commit: Ruby Desktop Agent (M0 Foundation)"')
run("git branch -D main")
run("git branch -m main")
run("git gc --aggressive --prune=all")
run("git count-objects -vH")
