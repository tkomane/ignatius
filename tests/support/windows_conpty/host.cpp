// Test-only Win32 ConPTY ownership/observation host. No product entry points.
#include "protocol.hpp"
#ifndef _WIN32
#include <iostream>
#include <memory>
int main() { std::cerr << "W07-064 native host requires actual Windows; no native evidence\n"; return 77; }
#else
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#define _WIN32_WINNT 0x0A00
#include <windows.h>
#include <sddl.h>
#include <atomic>
#include <chrono>
#include <cstring>
#include <filesystem>
#include <iostream>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <thread>
#include <utility>
using namespace std::chrono_literals;
namespace {
struct Handle {
    HANDLE value{INVALID_HANDLE_VALUE};
    Handle() = default;
    explicit Handle(HANDLE h) : value(h) {}
    Handle(const Handle&) = delete; Handle& operator=(const Handle&) = delete;
    Handle(Handle&& other) noexcept : value(std::exchange(other.value, INVALID_HANDLE_VALUE)) {}
    Handle& operator=(Handle&& other) noexcept { if (this != &other) { close(); value = std::exchange(other.value, INVALID_HANDLE_VALUE); } return *this; }
    ~Handle() { close(); }
    void close() { if (value && value != INVALID_HANDLE_VALUE) CloseHandle(value); value = INVALID_HANDLE_VALUE; }
    explicit operator bool() const { return value && value != INVALID_HANDLE_VALUE; }
};
[[noreturn]] void fail(const char* text) { throw std::runtime_error(std::string(text) + " native_error=" + std::to_string(GetLastError())); }
void require(bool okay, const char* text) { if (!okay) fail(text); }
std::uint64_t ticks() { LARGE_INTEGER n{}; require(QueryPerformanceCounter(&n) != 0, "QueryPerformanceCounter"); return static_cast<std::uint64_t>(n.QuadPart); }
std::uint64_t frequency() { LARGE_INTEGER n{}; require(QueryPerformanceFrequency(&n) != 0, "QueryPerformanceFrequency"); return static_cast<std::uint64_t>(n.QuadPart); }
using Clock = std::chrono::steady_clock;
DWORD remaining(Clock::time_point deadline, DWORD cap = 1000) { auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(deadline - Clock::now()).count(); return ms <= 0 ? 0 : static_cast<DWORD>((std::min)(ms, static_cast<long long>(cap))); }
std::string ascii(const std::wstring& value) { std::string result; for (wchar_t ch : value) { require(ch > 0 && ch <= 127, "controlled ASCII argument"); result += static_cast<char>(ch); } return result; }
bool token(const std::wstring& value) { return !value.empty() && value.size() <= 64 && value.find_first_not_of(L"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-") == std::wstring::npos; }
std::wstring quote(const std::wstring& value) { // Windows CRT argv quoting, including trailing backslashes.
    std::wstring out = L"\""; std::size_t slashes = 0;
    for (auto ch : value) { if (ch == L'\\') { ++slashes; continue; } if (ch == L'\"') { out.append(slashes * 2 + 1, L'\\'); out += ch; } else { out.append(slashes, L'\\'); out += ch; } slashes = 0; }
    out.append(slashes * 2, L'\\'); return out + L'\"';
}
std::wstring command(const std::vector<std::wstring>& args) { std::wstring out; for (const auto& arg : args) { if (!out.empty()) out += L' '; out += quote(arg); } return out; }
std::wstring image(HANDLE process) { std::wstring value(32768, L'\0'); DWORD n = static_cast<DWORD>(value.size()); require(QueryFullProcessImageNameW(process, 0, value.data(), &n) != 0, "QueryFullProcessImageNameW"); value.resize(n); return value; }
bool same_path(const std::wstring& a, const std::wstring& b) { return CompareStringOrdinal(a.c_str(), -1, b.c_str(), -1, TRUE) == CSTR_EQUAL; }
std::wstring self() { std::wstring value(32768, L'\0'); const auto n = GetModuleFileNameW(nullptr, value.data(), static_cast<DWORD>(value.size())); require(n > 0 && n < value.size(), "GetModuleFileNameW"); value.resize(n); return value; }
void exact_job(HANDLE process, HANDLE job) { BOOL inside = FALSE; require(IsProcessInJob(process, job, &inside) && inside, "exact owned Job membership"); }
FILETIME birth(HANDLE process) { FILETIME c{}, e{}, k{}, u{}; require(GetProcessTimes(process, &c, &e, &k, &u) != 0, "GetProcessTimes"); return c; }
std::vector<wchar_t> environment(const std::wstring& work) {
    wchar_t windows[32768]{}; const auto n = GetWindowsDirectoryW(windows, 32768); require(n > 0 && n < 32768, "GetWindowsDirectoryW");
    // No inherited environment block, HOME assignment, provider, log or PG route.
    std::vector<std::wstring> vars{L"IGNATIUS_CONFIG_DIR=" + work + L"\\config", L"IGNATIUS_DATA_DIR=" + work + L"\\data", L"PGPASSFILE=" + work + L"\\absent-passfile", L"SystemRoot=" + std::wstring(windows), L"TEMP=" + work, L"TMP=" + work, L"WINDIR=" + std::wstring(windows)};
    std::vector<wchar_t> out; for (const auto& var : vars) { out.insert(out.end(), var.begin(), var.end()); out.push_back(0); } out.push_back(0); return out;
}
struct Child { Handle process, thread; DWORD pid{}; };
Child create(const std::wstring& executable, std::vector<std::wstring> args, const std::wstring& work, STARTUPINFOW* startup, DWORD flags) {
    require(std::filesystem::path(executable).is_absolute() && std::filesystem::is_regular_file(executable), "absolute executable");
    require((flags & CREATE_BREAKAWAY_FROM_JOB) == 0 && (flags & CREATE_NEW_CONSOLE) == 0, "no breakaway or fresh/unowned console flags");
    args.insert(args.begin(), executable); auto line = command(args); auto env = environment(work); PROCESS_INFORMATION pi{};
    require(CreateProcessW(executable.c_str(), line.data(), nullptr, nullptr, FALSE, flags | CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED, env.data(), work.c_str(), startup, &pi) != 0, "CreateProcessW suspended");
    return {Handle(pi.hProcess), Handle(pi.hThread), pi.dwProcessId};
}
void resume(Child& child) { require(ResumeThread(child.thread.value) == 1, "ResumeThread exactly suspended"); child.thread.close(); }
// A client blocks only inside the contained Job; the outside supervisor owns
// the deadline and kills the exact Job if a client/pipe/console teardown stalls.
void send(const std::wstring& pipe, const std::string& line) {
    require(line.size() < w07::line_limit, "client line bound");
    const auto deadline = Clock::now() + 20s; Handle channel;
    while (!channel) {
        channel = Handle(CreateFileW(pipe.c_str(), GENERIC_READ | GENERIC_WRITE, 0, nullptr, OPEN_EXISTING, 0, nullptr));
        if (channel) break;
        require(GetLastError() == ERROR_PIPE_BUSY && Clock::now() < deadline, "owned control pipe open");
        WaitNamedPipeW(pipe.c_str(), (std::min)(remaining(deadline), DWORD{100}));
    }
    const auto message = line + "\n"; DWORD n{};
    require(WriteFile(channel.value, message.data(), static_cast<DWORD>(message.size()), &n, nullptr) && n == message.size(), "control write");
    char response[4]{}; DWORD used = 0;
    while (used < sizeof(response)) { require(ReadFile(channel.value, response + used, static_cast<DWORD>(sizeof(response) - used), &n, nullptr) && n > 0, "control acknowledgement"); used += n; }
    require(std::string_view(response, sizeof(response)) == "ACK\n", "control ACK contents");
    require(WriteFile(channel.value, "K\n", 2, &n, nullptr) && n == 2, "client ACK receipt");
}
struct Security {
    PSECURITY_DESCRIPTOR descriptor{};
    SECURITY_ATTRIBUTES attributes{};
    Security() {
        Handle token_handle; HANDLE raw{}; require(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw) != 0, "OpenProcessToken owner"); token_handle = Handle(raw);
        DWORD n{}; GetTokenInformation(raw, TokenUser, nullptr, 0, &n); require(n > 0 && n < 65536, "owner SID bound");
        std::vector<unsigned char> bytes(n); require(GetTokenInformation(raw, TokenUser, bytes.data(), n, &n) != 0, "GetTokenInformation owner SID");
        LPWSTR sid{}; require(ConvertSidToStringSidW(reinterpret_cast<TOKEN_USER*>(bytes.data())->User.Sid, &sid) != 0, "ConvertSidToStringSidW");
        const std::wstring sddl = L"D:P(A;;GA;;;SY)(A;;GA;;;" + std::wstring(sid) + L")"; LocalFree(sid);
        require(ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.c_str(), SDDL_REVISION_1, &descriptor, nullptr) != 0, "protected owner/SYSTEM DACL");
        attributes = {sizeof(attributes), descriptor, FALSE};
    }
    ~Security() { if (descriptor) LocalFree(descriptor); }
};
class Server {
    // Every kernel-referenced byte, OVERLAPPED and event is heap-owned together.
    // There is only one operation in flight in this serial fixed protocol.
    struct State {
        Handle pipe, event;
        OVERLAPPED ov{};
        std::array<char, 4> buffer{};
        DWORD transferred{};
        bool pending{};
    };
    std::unique_ptr<State> state_{std::make_unique<State>()};
    Clock::time_point cleanup_deadline_;
    w07::Budget budget_{w07::observation_limit, 0};
    OVERLAPPED* begin() {
        require(!state_->pending, "one serial control I/O operation");
        state_->ov = {}; state_->transferred = 0; state_->ov.hEvent = state_->event.value;
        require(ResetEvent(state_->event.value) != 0, "control event reset");
        return &state_->ov;
    }
    DWORD pending(Clock::time_point deadline) {
        state_->pending = true;
        const auto wait = WaitForSingleObject(state_->event.value, remaining(deadline, 25000));
        if (wait != WAIT_OBJECT_0) {
            CancelIoEx(state_->pipe.value, &state_->ov);
            // Cancellation is a request, never proof of completion. Reserve
            // only remaining absolute cleanup time and never block in GOR(TRUE).
            const auto cancelled = WaitForSingleObject(state_->event.value, remaining(cleanup_deadline_, 500));
            DWORD ignored{}; const bool okay = GetOverlappedResult(state_->pipe.value, &state_->ov, &ignored, FALSE) != 0;
            const auto error = okay ? ERROR_SUCCESS : GetLastError();
            const bool complete = okay || error == ERROR_OPERATION_ABORTED || (cancelled == WAIT_OBJECT_0 && error != ERROR_IO_INCOMPLETE);
            if (complete) state_->pending = false;
            SetLastError(ERROR_TIMEOUT);
            fail(complete ? "control I/O timeout; cancellation completion confirmed" : "control I/O timeout; cancellation unconfirmed; operation storage retained until supervisor process exit");
        }
        DWORD n{}; const bool okay = GetOverlappedResult(state_->pipe.value, &state_->ov, &n, FALSE) != 0;
        const auto error = okay ? ERROR_SUCCESS : GetLastError();
        if (okay || error != ERROR_IO_INCOMPLETE) state_->pending = false;
        SetLastError(error); require(okay, "control I/O completion"); return n;
    }
    DWORD read_bytes(char* into, DWORD count, Clock::time_point deadline) {
        auto* ov = begin();
        const bool okay = ReadFile(state_->pipe.value, into, count, &state_->transferred, ov) != 0;
        DWORD n = state_->transferred;
        if (!okay) { require(GetLastError() == ERROR_IO_PENDING, "control read"); n = pending(deadline); }
        require(n > 0 && budget_.add(n), "structured control byte budget"); return n;
    }
public:
    Server(const std::wstring& name, Security& security, Clock::time_point cleanup_deadline) : cleanup_deadline_(cleanup_deadline) {
        state_->event = Handle(CreateEventW(nullptr, TRUE, FALSE, nullptr)); require(static_cast<bool>(state_->event), "control event");
        state_->pipe = Handle(CreateNamedPipeW(name.c_str(), PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, 1, 4096, 4096, 0, &security.attributes)); require(static_cast<bool>(state_->pipe), "local first-instance pipe");
    }
    ~Server() {
        if (state_->pending) {
            // A failed cancellation must not let C++ destructors free live I/O
            // storage or close its event. This one bounded state, pipe, event
            // and buffers survive until OS supervisor-process teardown. Failure
            // is explicit; this is not confirmed cleanup or reusable storage.
            (void)state_.release();
        }
    }
    void connect(Clock::time_point deadline) {
        auto* ov = begin();
        if (!ConnectNamedPipe(state_->pipe.value, ov)) {
            const auto error = GetLastError(); if (error == ERROR_PIPE_CONNECTED) return;
            require(error == ERROR_IO_PENDING, "ConnectNamedPipe"); (void)pending(deadline);
        }
    }
    DWORD client() { ULONG pid{}; require(GetNamedPipeClientProcessId(state_->pipe.value, &pid) && pid != 0, "control client process identity"); return pid; }
    std::string read(Clock::time_point deadline) {
        std::string line;
        for (;;) {
            require(read_bytes(state_->buffer.data(), 1, deadline) == 1, "control read byte");
            const char ch = state_->buffer[0]; if (ch == '\n') return line;
            require(line.size() < w07::line_limit, "control line limit"); line += ch;
        }
    }
    void ack(Clock::time_point deadline) {
        std::copy_n("ACK\n", 4, state_->buffer.data()); auto* ov = begin();
        const bool okay = WriteFile(state_->pipe.value, state_->buffer.data(), 4, &state_->transferred, ov) != 0;
        DWORD n = state_->transferred;
        if (!okay) { require(GetLastError() == ERROR_IO_PENDING, "control ACK write"); n = pending(deadline); }
        require(n == 4 && budget_.add(4), "ACK budget");
        // A bounded client receipt proves ACK was consumed before disconnect.
        DWORD used = 0; while (used < 2) used += read_bytes(state_->buffer.data() + used, 2 - used, deadline);
        require(std::string_view(state_->buffer.data(), 2) == "K\n", "ACK receipt contents");
        const bool disconnected = DisconnectNamedPipe(state_->pipe.value) != 0; require(disconnected || GetLastError() == ERROR_PIPE_NOT_CONNECTED, "control disconnect");
    }
    bool quiescent() const { return !state_->pending; }
    std::uint64_t bytes() const { return budget_.used; }
};
struct Console {
    HPCON pseudo{}; Handle input_write, output_read, capture; std::thread reader;
    std::atomic<std::uint64_t> seen{0}, kept{0}; std::atomic<DWORD> output_error{0}; bool joined{}, closed{};
    explicit Console(const std::wstring& file) {
        capture = Handle(CreateFileW(file.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr)); require(static_cast<bool>(capture), "exclusive translated capture");
        HANDLE ir{}, iw{}, oraw{}, ow{}; require(CreatePipe(&ir, &iw, nullptr, 0) != 0, "ConPTY input pipe"); Handle input_read(ir); input_write = Handle(iw);
        require(CreatePipe(&oraw, &ow, nullptr, 0) != 0, "ConPTY output pipe"); output_read = Handle(oraw); Handle output_write(ow);
        const auto result = CreatePseudoConsole(COORD{80, 24}, input_read.value, output_write.value, 0, &pseudo); require(SUCCEEDED(result), "CreatePseudoConsole flags zero");
        input_read.close(); output_write.close();
        reader = std::thread([this] {
            char bytes[4096];
            for (;;) {
                DWORD n{}; const bool okay = ReadFile(output_read.value, bytes, sizeof(bytes), &n, nullptr) != 0;
                const auto error = okay ? ERROR_SUCCESS : GetLastError();
                const auto failure = w07::pipe_read_failure(okay, n, error);
                if (failure != 0) { output_error = failure; break; }
                if (n == 0) break; // Only successful EOF or known broken pipe.

                const auto prior = seen.fetch_add(n); const auto room = prior >= w07::output_limit ? 0 : w07::output_limit - prior;
                const auto keep = static_cast<DWORD>((std::min)(room, static_cast<std::uint64_t>(n)));
                DWORD offset = 0;
                while (offset < keep) { DWORD written{}; if (!WriteFile(capture.value, bytes + offset, keep - offset, &written, nullptr) || written == 0) { output_error = GetLastError(); if (output_error == 0) output_error = ERROR_WRITE_FAULT; break; } offset += written; kept.fetch_add(written); }
                if (prior > UINT64_MAX - n || prior + n > w07::output_limit) output_error = ERROR_BUFFER_OVERFLOW;
            }
        });
    }
    void cleanup() { input_write.close(); if (pseudo) { ClosePseudoConsole(pseudo); pseudo = nullptr; } closed = true; if (reader.joinable()) reader.join(); joined = true; output_read.close(); capture.close(); }
    ~Console() { cleanup(); } // Outside 25 s supervisor contains blocked native teardown.
};
struct Attributes {
    std::vector<unsigned char> bytes; LPPROC_THREAD_ATTRIBUTE_LIST list{};
    explicit Attributes(HPCON pseudo) {
        SIZE_T n{}; InitializeProcThreadAttributeList(nullptr, 1, 0, &n); require(n > 0 && n < 65536, "attribute storage bound"); bytes.resize(n); list = reinterpret_cast<LPPROC_THREAD_ATTRIBUTE_LIST>(bytes.data());
        require(InitializeProcThreadAttributeList(list, 1, 0, &n) != 0, "InitializeProcThreadAttributeList");
        if (!UpdateProcThreadAttribute(list, 0, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, pseudo, sizeof(pseudo), nullptr, nullptr)) { DeleteProcThreadAttributeList(list); list = nullptr; fail("pseudoconsole attribute"); }
    }
    ~Attributes() { if (list) DeleteProcThreadAttributeList(list); }
};
int worker(const std::wstring& kind, const std::wstring& guard, const std::wstring& pipe, const std::wstring& nonce, const std::wstring& sentinel, const std::wstring& work) {
    Console console(work + L"\\translated.bin"); Attributes attributes(console.pseudo); STARTUPINFOEXW si{}; si.StartupInfo.cb = sizeof(si); si.lpAttributeList = attributes.list;
    auto child = create(guard, {kind, self(), pipe, nonce, sentinel}, work, &si.StartupInfo, EXTENDED_STARTUPINFO_PRESENT);
    const auto created = birth(child.process.value);
    send(pipe, "R " + ascii(nonce) + " " + std::to_string(child.pid) + " " + std::to_string(created.dwLowDateTime) + " " + std::to_string(created.dwHighDateTime));
    // Supervisor authenticated and checked this exact child in its exact Job.
    resume(child); const auto resumed = ticks(); send(pipe, "S " + ascii(nonce) + " " + std::to_string(static_cast<std::uint32_t>(resumed)) + " " + std::to_string(static_cast<std::uint32_t>(resumed >> 32)));
    require(WaitForSingleObject(child.process.value, INFINITE) == WAIT_OBJECT_0, "owned guard wait"); DWORD exit{}; require(GetExitCodeProcess(child.process.value, &exit) != 0, "owned guard exit");
    console.cleanup();
    const auto seen = console.seen.load(); const auto kept = console.kept.load(); require(seen <= UINT32_MAX && kept <= UINT32_MAX, "output counter overflow");
    send(pipe, "C " + ascii(nonce) + " " + std::to_string(exit) + " " + std::to_string(seen) + " " + std::to_string(kept) + " " + std::to_string(console.output_error.load()) + " " + std::to_string(console.joined) + " " + std::to_string(console.closed));
    return 0;
}
const char* phase_name(w07::Phase p) { return p == w07::Phase::baseline ? "baseline" : p == w07::Phase::acquired ? "acquired" : "restored"; }
int probe(const std::wstring& pipe, const std::wstring& nonce, const std::wstring& sentinel, const std::wstring& operation, const std::wstring& phase, DWORD guard_pid) {
    require(token(nonce) && token(sentinel), "probe controlled token");
    // Positive console attachment BEFORE opening/mutating native console state.
    // The worker hosts HPCON but never attaches or observes it. This process
    // inherited the Rust client's actual pseudoconsole, not the runner console.
    DWORD members[128]{}; const auto count = GetConsoleProcessList(members, 128);
    require(count > 0 && count <= 128 && guard_pid != GetCurrentProcessId(), "bounded attached-console process list");
    const bool guard_found = std::find(members, members + count, guard_pid) != members + count;
    const bool self_found = std::find(members, members + count, GetCurrentProcessId()) != members + count;
    require(guard_found && self_found, "observer shares actual Rust child console");
    send(pipe, "A " + ascii(nonce) + " " + std::to_string(guard_pid) + " " + std::to_string(count) + " " + std::to_string(guard_found) + " " + std::to_string(self_found));
    Handle input(CreateFileW(L"CONIN$", GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, 0, nullptr));
    Handle output(CreateFileW(L"CONOUT$", GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, 0, nullptr));
    require(input && output, "actual CONIN$/CONOUT$ handles");
    if (operation == L"seed") {
        DWORD in{}, out{}; require(GetConsoleMode(input.value, &in) && GetConsoleMode(output.value, &out), "native baseline modes");
        in |= ENABLE_PROCESSED_INPUT | ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_EXTENDED_FLAGS;
        in &= ~(ENABLE_MOUSE_INPUT | ENABLE_WINDOW_INPUT | ENABLE_QUICK_EDIT_MODE | ENABLE_VIRTUAL_TERMINAL_INPUT);
        require(SetConsoleMode(input.value, in) && SetConsoleMode(output.value, out | ENABLE_VIRTUAL_TERMINAL_PROCESSING), "seed owned baseline modes");
        CONSOLE_CURSOR_INFO cursor{}; require(GetConsoleCursorInfo(output.value, &cursor) != 0, "baseline cursor query"); cursor.bVisible = TRUE; require(SetConsoleCursorInfo(output.value, &cursor) != 0, "seed visible cursor");
        DWORD n{}; // Buffer support is optional, and is reported independently below.
        WriteConsoleOutputCharacterW(output.value, sentinel.data(), static_cast<DWORD>(sentinel.size()), COORD{0, 0}, &n);
    } else if (operation == L"perturb") {
        require(SetConsoleMode(input.value, ENABLE_EXTENDED_FLAGS | ENABLE_WINDOW_INPUT | ENABLE_MOUSE_INPUT) != 0, "known unrestored native mode");
    } else require(operation == L"observe", "probe operation allowlist");
    DWORD in{}, out{}, error{}; CONSOLE_CURSOR_INFO cursor{};
    const bool input_ok = GetConsoleMode(input.value, &in) != 0; if (!input_ok) error = GetLastError();
    const bool output_ok = GetConsoleMode(output.value, &out) != 0; if (!output_ok && !error) error = GetLastError();
    const bool cursor_ok = GetConsoleCursorInfo(output.value, &cursor) != 0; if (!cursor_ok && !error) error = GetLastError();
    std::wstring found(sentinel.size(), L'\0'); DWORD n{}; const bool buffer_ok = ReadConsoleOutputCharacterW(output.value, found.data(), static_cast<DWORD>(found.size()), COORD{0, 0}, &n) != 0 && n == found.size();
    if (!buffer_ok && !error) error = GetLastError();
    require(phase == L"baseline" || phase == L"acquired" || phase == L"restored", "probe phase allowlist");
    std::ostringstream line; line << "O " << ascii(nonce) << ' ' << ascii(phase) << ' ' << input_ok << ' ' << output_ok << ' ' << cursor_ok << ' ' << buffer_ok << ' ' << in << ' ' << out << ' ' << (cursor.bVisible != FALSE) << ' ' << (buffer_ok && found == sentinel) << ' ' << error;
    send(pipe, line.str()); return 0;
}
struct Receipt {
    std::uint64_t start{ticks()}, registered{}, resumed{}, ready{}, killed{}, waited{}, cleanup{}, end{};
    DWORD child_pid{}, child_exit{}, worker_exit{}, output_error{}, job_remaining{UINT32_MAX}; std::uint32_t seen{}, kept{};
    std::uint32_t attachment_attestations{};
    bool unassigned_terminated{}, unassigned_waited{};
    bool ready_ack{}, terminated{}, guard_waited{}, clean{}, worker_waited{}, owned_worker{}, owned_guard{}, deadline_elapsed{}, success{};
    std::string failure; std::vector<w07::Observation> observations;
};
void emit(const Receipt& r, w07::Case kind, std::uint64_t bytes, bool io_quiescent) {
    std::cout << "{\"schema\":1,\"case\":\"" << w07::case_name(kind) << "\",\"native_windows\":true,\"success\":" << (r.success ? "true" : "false") << ",\"failure\":\"";
    for (char ch : r.failure) std::cout << (ch == '"' || ch == '\\' || static_cast<unsigned char>(ch) < 32 ? '_' : ch);
    std::cout << "\",\"qpc_frequency\":" << frequency() << ",\"clocks\":{\"start\":" << r.start << ",\"registered\":" << r.registered << ",\"resume_ack\":" << r.resumed << ",\"hang_ready_ack\":" << r.ready << ",\"kill\":" << r.killed << ",\"guard_wait\":" << r.waited << ",\"cleanup\":" << r.cleanup << ",\"end\":" << r.end << "},\"child_pid\":" << r.child_pid << ",\"child_exit\":" << r.child_exit << ",\"worker_exit\":" << r.worker_exit << ",\"exact_job_worker\":" << r.owned_worker << ",\"exact_job_guard\":" << r.owned_guard << ",\"unassigned_terminated\":" << r.unassigned_terminated << ",\"unassigned_waited\":" << r.unassigned_waited << ",\"hang_ready_ack\":" << r.ready_ack << ",\"hang_deadline_elapsed\":" << r.deadline_elapsed << ",\"terminated_owned\":" << r.terminated << ",\"guard_waited\":" << r.guard_waited << ",\"worker_waited\":" << r.worker_waited << ",\"cleanup_confirmed\":" << r.clean << ",\"job_active_processes\":" << r.job_remaining << ",\"output_seen\":" << r.seen << ",\"output_kept\":" << r.kept << ",\"output_error\":" << r.output_error << ",\"attachment_attestations\":" << r.attachment_attestations << ",\"control_io_cleanup_confirmed\":" << io_quiescent << ",\"control_bytes\":" << bytes << ",\"paste_observation\":\"open-not-measured\",\"alternate_buffer\":\"";
    bool alternate = r.observations.size() == 3 && r.observations[0].buffer_ok && r.observations[1].buffer_ok && r.observations[2].buffer_ok && r.observations[0].sentinel_seen && !r.observations[1].sentinel_seen && r.observations[2].sentinel_seen;
    std::cout << (kind == w07::Case::capability && alternate ? "native-transition-observed" : "open-not-proven") << "\",\"observations\":[";
    bool first = true; for (const auto& o : r.observations) { if (!first) std::cout << ','; first = false; std::cout << "{\"phase\":\"" << phase_name(o.phase) << "\",\"input_ok\":" << o.input_ok << ",\"output_ok\":" << o.output_ok << ",\"cursor_ok\":" << o.cursor_ok << ",\"buffer_ok\":" << o.buffer_ok << ",\"input_mask\":" << o.input << ",\"output_mask\":" << o.output << ",\"cursor_visible\":" << o.cursor_visible << ",\"sentinel_seen\":" << o.sentinel_seen << ",\"native_error\":" << o.native_error << '}'; }
    std::cout << "]}\n";
}
int supervise(w07::Case kind, const std::wstring& guard, const std::wstring& nonce, const std::wstring& work) {
    Receipt r; const auto outer = Clock::now() + 25s; const auto work_deadline = outer - 2s; auto phase_deadline = Clock::now() + 20s;
    // This non-inheritable Job handle is unnamed and NEVER passed/duplicated to
    // worker, guard or probe. Only the outside supervisor retains it.
    Handle job(CreateJobObjectW(nullptr, nullptr)); require(static_cast<bool>(job), "CreateJobObjectW"); JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{}; limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE; require(SetInformationJobObject(job.value, JobObjectExtendedLimitInformation, &limits, sizeof(limits)) != 0, "kill-on-close Job");
    Handle guard_process, owned_probe; bool probe_attested = false; Child child; Security security; const std::wstring pipe_name = L"\\\\.\\pipe\\ignatius-w07-064-" + nonce; Server server(pipe_name, security, outer);
    try {
        STARTUPINFOW si{}; si.cb = sizeof(si); const std::wstring sentinel = L"W07-" + nonce;
        child = create(self(), {L"--worker", std::wstring(w07::case_name(kind), w07::case_name(kind) + strlen(w07::case_name(kind))), guard, pipe_name, nonce, sentinel, work}, work, &si, CREATE_NO_WINDOW);
        require(AssignProcessToJobObject(job.value, child.process.value) != 0, "assign suspended worker to owned Job"); exact_job(child.process.value, job.value); r.owned_worker = true; resume(child);
        for (;;) {
            require(Clock::now() < (std::min)(work_deadline, phase_deadline), "setup/case/outer deadline");
            server.connect((std::min)(work_deadline, phase_deadline));
            const auto pid = server.client(); Handle sender(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, FALSE, pid)); require(static_cast<bool>(sender), "stable pipe client handle"); exact_job(sender.value, job.value); const auto sender_image = image(sender.value);
            const auto line = server.read((std::min)(work_deadline, phase_deadline)); const auto parts = w07::words(line); require(parts.size() >= 2 && parts[1] == ascii(nonce), "control nonce/shape");
            const bool from_worker = pid == child.pid && same_path(sender_image, self());
            if (parts[0] == "R") {
                require(from_worker && parts.size() == 5 && !guard_process, "single worker registration"); std::uint32_t n{}, low{}, high{}; require(w07::number(parts[2], n) && w07::number(parts[3], low) && w07::number(parts[4], high), "registration integers");
                guard_process = Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_DUP_HANDLE | PROCESS_TERMINATE | SYNCHRONIZE, FALSE, n)); require(static_cast<bool>(guard_process), "stable owned guard process"); exact_job(guard_process.value, job.value); require(same_path(image(guard_process.value), guard), "guard image identity"); const auto b = birth(guard_process.value); require(b.dwLowDateTime == low && b.dwHighDateTime == high, "guard creation identity"); r.child_pid = n; r.owned_guard = true; r.registered = ticks();
            } else if (parts[0] == "S") {
                require(from_worker && parts.size() == 4 && guard_process && !r.resumed, "resume acknowledgement"); std::uint32_t low{}, high{};
                require(w07::number(parts[2], low) && w07::number(parts[3], high), "actual resume QPC"); r.resumed = (static_cast<std::uint64_t>(high) << 32) | low;
                require(r.resumed >= r.registered && r.resumed <= ticks(), "actual shared-clock resume interval");
            }
            else if (parts[0] == "H") { require(kind == w07::Case::hung && pid == r.child_pid && same_path(sender_image, guard) && parts.size() == 2 && !r.ready_ack && w07::required_phase_order(kind, r.observations), "positive HANG_READY identity/order"); }
            else if (parts[0] == "P") {
                require(pid == r.child_pid && same_path(sender_image, guard) && parts.size() == 5 && !owned_probe, "Rust actual observer handle registration");
                std::uint32_t observer_pid{}, low{}, high{};
                require(w07::number(parts[2], observer_pid) && w07::number(parts[3], low) && w07::number(parts[4], high), "observer handle integers");
                const std::uint64_t raw = (static_cast<std::uint64_t>(high) << 32) | low;
                require(raw != 0 && raw <= static_cast<std::uint64_t>(UINTPTR_MAX), "observer native handle width"); HANDLE duplicate{};
                require(DuplicateHandle(guard_process.value, reinterpret_cast<HANDLE>(static_cast<UINT_PTR>(raw)), GetCurrentProcess(), &duplicate, PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, FALSE, 0) != 0, "duplicate actual Rust Child observer handle");
                owned_probe = Handle(duplicate); exact_job(owned_probe.value, job.value);
                require(GetProcessId(owned_probe.value) == observer_pid && same_path(image(owned_probe.value), self()) && WaitForSingleObject(owned_probe.value, 0) == WAIT_TIMEOUT, "live exact owned observer handle/image");
                probe_attested = false;
            }
            else if (parts[0] == "A") {
                require(!from_worker && same_path(sender_image, self()) && guard_process && parts.size() == 6 && owned_probe && GetProcessId(owned_probe.value) == pid && !probe_attested, "actual Child inside-console observer identity");
                std::uint32_t guard_pid{}, count{}, guard_found{}, self_found{};
                require(w07::number(parts[2], guard_pid) && w07::number(parts[3], count) && w07::number(parts[4], guard_found) && w07::number(parts[5], self_found), "attachment integers");
                require(guard_pid == r.child_pid && count >= 2 && count <= 128 && guard_found == 1 && self_found == 1, "positive shared owned-console attachment");
                const auto actual_birth = birth(owned_probe.value); const auto pipe_birth = birth(sender.value); require(actual_birth.dwLowDateTime == pipe_birth.dwLowDateTime && actual_birth.dwHighDateTime == pipe_birth.dwHighDateTime, "exact pipe client process creation identity");
                probe_attested = true; ++r.attachment_attestations;
            }
            else if (parts[0] == "O") { require(!from_worker && same_path(sender_image, self()) && guard_process && owned_probe && probe_attested && GetProcessId(owned_probe.value) == pid, "attested native probe sender identity"); const auto observed = w07::parse_observation(line, ascii(nonce)); require(observed.has_value() && r.observations.size() < 3, "bounded native observation"); r.observations.push_back(*observed); owned_probe.close(); probe_attested = false; }
            else if (parts[0] == "C") {
                require(from_worker && parts.size() == 8 && guard_process, "worker cleanup receipt"); std::array<std::uint32_t, 6> n{}; for (std::size_t i = 0; i < n.size(); ++i) require(w07::number(parts[i + 2], n[i]), "cleanup integers");
                r.child_exit = n[0]; r.seen = n[1]; r.kept = n[2]; r.output_error = n[3]; r.clean = n[4] == 1 && n[5] == 1;
                require(WaitForSingleObject(guard_process.value, 0) == WAIT_OBJECT_0, "guard actually exited before cleanup"); r.guard_waited = true; if (!r.waited) r.waited = ticks(); DWORD actual{}; require(GetExitCodeProcess(guard_process.value, &actual) && actual == r.child_exit, "matching actual guard exit"); r.cleanup = ticks();
            } else { fail("control message allowlist"); }
            server.ack((std::min)(work_deadline, phase_deadline));
            if (parts[0] == "H") {
                r.ready_ack = true; r.ready = ticks();
                // This is a direct wait on the stable, acknowledged child, not
                // a detached PID timer or a sleep used to infer readiness.
                require(WaitForSingleObject(guard_process.value, 250) == WAIT_TIMEOUT, "acknowledged owned child remained hung for 250 ms");
                require(ticks() - r.ready >= frequency() / 4, "monotonic positive hang interval"); r.deadline_elapsed = true;
                exact_job(guard_process.value, job.value); require(TerminateProcess(guard_process.value, 124) != 0, "terminate stable owned hung guard"); r.terminated = true; r.killed = ticks();
                require(WaitForSingleObject(guard_process.value, remaining(outer, 1000)) == WAIT_OBJECT_0, "wait owned hung guard"); r.guard_waited = true; r.waited = ticks(); phase_deadline = work_deadline;
            }
            if (parts[0] == "O" && kind != w07::Case::hung && w07::required_phase_order(kind, r.observations)) {
                // The final observer has received its ACK. Bound the actual
                // Rust process separately from the worker's console teardown.
                require(WaitForSingleObject(guard_process.value, remaining(phase_deadline, 20000)) == WAIT_OBJECT_0, "real guard child 20 s deadline");
                r.guard_waited = true; r.waited = ticks(); phase_deadline = work_deadline;
            }
            if (parts[0] == "C") break;
        }
        require(WaitForSingleObject(child.process.value, remaining(outer, 2000)) == WAIT_OBJECT_0, "wait owned worker"); r.worker_waited = true; require(GetExitCodeProcess(child.process.value, &r.worker_exit) != 0, "worker exit");
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION accounting{}; require(QueryInformationJobObject(job.value, JobObjectBasicAccountingInformation, &accounting, sizeof(accounting), nullptr) != 0, "owned Job accounting"); r.job_remaining = accounting.ActiveProcesses;
        while (r.job_remaining != 0 && Clock::now() < outer) {
            // Closing HPCON may complete asynchronously on newer Windows.
            // Poll the actual owned Job count; elapsed time never proves exit.
            Sleep(1); require(QueryInformationJobObject(job.value, JobObjectBasicAccountingInformation, &accounting, sizeof(accounting), nullptr) != 0, "final owned Job accounting"); r.job_remaining = accounting.ActiveProcesses;
        }
        require(r.clean && r.worker_exit == 0 && r.job_remaining == 0 && r.output_error == 0 && r.seen <= w07::output_limit && r.kept == r.seen, "complete bounded cleanup/output");
        require(w07::required_phase_order(kind, r.observations) && r.attachment_attestations == r.observations.size() && w07::baseline_valid(r.observations[0]), "native phase order/cooked baseline");
        if (kind == w07::Case::hung) require(r.child_exit == 124 && w07::hung_proven(r.ready_ack, r.deadline_elapsed, r.terminated, r.guard_waited, r.clean), "acknowledged owned hung-child control");
        else {
            require(r.child_exit == 0, "controlled guard child exit");
            if (kind == w07::Case::unrestored) require(!w07::restored(r.observations.front(), r.observations.back()) && r.observations.back().input_ok && r.observations.back().output_ok && r.observations.back().cursor_ok && w07::raw(r.observations.back()), "known unrestored mode detected");
            else { require(w07::restored(r.observations.front(), r.observations.back()), "native restoration of seeded baseline"); const auto& acquired = r.observations[1]; if (kind == w07::Case::no_guard) require(w07::restored(r.observations.front(), acquired), "no-guard stable control"); else require(acquired.input_ok && acquired.output_ok && acquired.cursor_ok && w07::raw(acquired) && (acquired.input & w07::mouse_bit) != 0 && !acquired.cursor_visible, "genuine raw/mouse/cursor acquisition capability"); }
        }
        r.success = true;
    } catch (const std::exception& error) {
        r.failure = error.what(); // Original assignment/validation error retained.
        if (child.process && !r.owned_worker) {
            // A created-but-unassigned suspended process is outside this Job.
            // Closing/terminating the Job alone cannot clean it up.
            r.unassigned_terminated = TerminateProcess(child.process.value, 125) != 0;
            r.unassigned_waited = WaitForSingleObject(child.process.value, remaining(outer, 1000)) == WAIT_OBJECT_0;
        }
        // Fail closed. No retry, subsequent case or runner-control bypass.
        TerminateJobObject(job.value, 125); // This exact new Job, never the runner Job.
        if (guard_process) { r.guard_waited = WaitForSingleObject(guard_process.value, remaining(outer, 1000)) == WAIT_OBJECT_0; }
        if (child.process) { r.worker_waited = WaitForSingleObject(child.process.value, remaining(outer, 1000)) == WAIT_OBJECT_0; }
    }
    r.end = ticks(); emit(r, kind, server.bytes(), server.quiescent()); return r.success ? 0 : 1;
}
}
int wmain(int argc, wchar_t** argv) {
    try {
        if (argc == 8 && std::wstring_view(argv[1]) == L"--worker") return worker(argv[2], argv[3], argv[4], argv[5], argv[6], argv[7]);
        if (argc == 8 && std::wstring_view(argv[1]) == L"--probe") { std::uint32_t pid{}; require(w07::number(ascii(argv[7]), pid), "controlled Rust PID"); return probe(argv[2], argv[3], argv[4], argv[5], argv[6], pid); }
        require(argc == 6 && std::wstring_view(argv[1]) == L"--supervise", "first capability/control invocation only");
        const auto kind = w07::parse_case(ascii(argv[2])); require(kind.has_value() && token(argv[4]), "case/nonce allowlist");
        require(std::filesystem::path(argv[5]).is_absolute() && std::filesystem::is_directory(argv[5]), "existing owned work directory");
        require(!std::filesystem::exists(std::filesystem::path(argv[5]) / L"absent-passfile"), "absent owned passfile");
        return supervise(*kind, argv[3], argv[4], argv[5]);
    } catch (const std::exception& error) { std::cerr << "W07-064 host: " << error.what() << '\n'; return 2; }
}
#endif
