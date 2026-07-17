-- Minimal bidirectional subprocess over pipe(2)+fork(2)+execvp(3).
--
-- Why this exists: Lua's io.popen is unidirectional, and glibc popen("r+")
-- does not give a working bidirectional handle on Linux. The shipsim engine
-- in --stdin mode needs a long-lived pipe where the parent writes NDJSON
-- orders/requests to the child's stdin and reads NDJSON responses from the
-- child's stdout. That requires two real pipes (one each direction), which
-- is exactly what Rust's Stdio::piped does in frontend/tui/src/harness.rs.
--
-- This module is Linux-specific (uses POSIX syscalls via luajit FFI). It is
-- luajit-clean (no love.* APIs) so it runs under plain luajit for tests.
--
-- API:
--   subprocess.spawn(argv)        -> proc | nil, err   (argv = array of str)
--   subprocess.write_line(proc, s) -> true | nil, err  (appends \n, flushes)
--   subprocess.read_line(proc)    -> line | nil        (nil on EOF)
--   subprocess.kill(proc)         (SIGTERM + reap)
--   subprocess.close(proc)        (close both pipe fds; does not signal)

local ffi = require("ffi")

ffi.cdef[[
  int pipe(int fds[2]);
  int fork(void);
  int dup2(int oldfd, int newfd);
  int close(int fd);
  int execvp(const char *file, char *const argv[]);
  int waitpid(int pid, int *status, int options);
  ssize_t read(int fd, void *buf, size_t count);
  ssize_t write(int fd, const void *buf, size_t count);
  int kill(int pid, int sig);
  int fcntl(int fd, int cmd, int arg);
  void *malloc(size_t size);
  void free(void *ptr);
  int open(const char *pathname, int flags, int mode);
]]

-- fcntl F_GETFL/F_SETFL to set O_NONBLOCK if needed; constants on Linux.
local F_GETFL = 3
local F_SETFL = 4
local O_NONBLOCK = 2048  -- 02000 on Linux

local subprocess = {}

--- Spawn a child running argv[0] with args argv[1..]. Returns a proc table
--- with .pid, .stdin_fd (parent writes here), .stdout_fd (parent reads here).
function subprocess.spawn(argv)
  local to_child = ffi.new("int[2]")    -- parent writes to_child[1], child reads to_child[0]
  local from_child = ffi.new("int[2]")  -- child writes from_child[1], parent reads from_child[0]
  if ffi.C.pipe(to_child) ~= 0 then
    return nil, "pipe(to_child) failed"
  end
  if ffi.C.pipe(from_child) ~= 0 then
    ffi.C.close(to_child[0]); ffi.C.close(to_child[1])
    return nil, "pipe(from_child) failed"
  end

  local pid = ffi.C.fork()
  if pid < 0 then
    ffi.C.close(to_child[0]); ffi.C.close(to_child[1])
    ffi.C.close(from_child[0]); ffi.C.close(from_child[1])
    return nil, "fork failed"
  end

  if pid == 0 then
    -- Child: wire up stdin/stdout, then exec.
    ffi.C.dup2(to_child[0], 0)       -- child stdin = read end of to_child
    ffi.C.dup2(from_child[1], 1)     -- child stdout = write end of from_child
    ffi.C.close(to_child[0]); ffi.C.close(to_child[1])
    ffi.C.close(from_child[0]); ffi.C.close(from_child[1])
    -- Build argv array (NULL-terminated, char*const[]).
    local cargv = ffi.new("char*[?]", #argv + 1)
    for i, a in ipairs(argv) do
      cargv[i - 1] = ffi.cast("char *", a)
    end
    cargv[#argv] = nil
    ffi.C.execvp(argv[1], cargv)
    -- execvp only returns on failure.
    os.exit(127)
  end

  -- Parent: close the ends the child uses.
  ffi.C.close(to_child[0])
  ffi.C.close(from_child[1])
  return {
    pid = pid,
    stdin_fd = to_child[1],    -- parent writes here
    stdout_fd = from_child[0], -- parent reads here
  }
end

--- Write a string + newline to the child stdin. Returns true or nil, err.
function subprocess.write_line(proc, s)
  local data = s .. "\n"
  local n = #data
  local cdata = ffi.cast("const char *", data)
  local written = 0
  while written < n do
    local w = ffi.C.write(proc.stdin_fd, cdata + written, n - written)
    if w < 0 then
      return nil, "write failed"
    end
    written = written + w
  end
  return true
end

--- Read one line (up to and including \n) from child stdout. Returns the
--- line without the trailing newline, or nil on EOF. Blocks until a line is
--- available or the child closes stdout.
function subprocess.read_line(proc)
  local chunks = {}
  local buf = ffi.new("char[?]", 4096)
  while true do
    local n = ffi.C.read(proc.stdout_fd, buf, 4096)
    if n == 0 then
      -- EOF
      if #chunks == 0 then return nil end
      break
    end
    if n < 0 then
      return nil
    end
    local chunk = ffi.string(buf, n)
    local nl = chunk:find("\n", 1, true)
    if nl then
      chunks[#chunks + 1] = chunk:sub(1, nl - 1)
      -- Note: any bytes after the newline in this chunk are dropped. The
      -- shipsim engine emits exactly one line per order/request, so there is
      -- never trailing data in practice.
      return table.concat(chunks)
    end
    chunks[#chunks + 1] = chunk
  end
  return table.concat(chunks)
end

--- SIGTERM the child and reap it. Closes both pipe fds.
function subprocess.kill(proc)
  if proc.pid and proc.pid > 0 then
    ffi.C.kill(proc.pid, 15) -- SIGTERM
    local status = ffi.new("int[1]")
    ffi.C.waitpid(proc.pid, status, 0)
    proc.pid = 0
  end
  subprocess.close(proc)
end

--- Close both pipe fds (no signal).
function subprocess.close(proc)
  if proc.stdin_fd and proc.stdin_fd >= 0 then
    ffi.C.close(proc.stdin_fd)
    proc.stdin_fd = -1
  end
  if proc.stdout_fd and proc.stdout_fd >= 0 then
    ffi.C.close(proc.stdout_fd)
    proc.stdout_fd = -1
  end
end

return subprocess
