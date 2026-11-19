# Read-only observation of the job process' own Windows session and desktop.
# The caller should branch on JSON status. An unavailable desktop is a result.
$ErrorActionPreference = 'Stop'

$source = @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class IgnatiusDesktopCapability
{
    private const int WtsConnectState = 8;
    private const int WtsActive = 0;
    private const int UoiFlags = 1;
    private const int UoiName = 2;
    private const uint WsfVisible = 1;
    private const uint DesktopReadObjects = 1;

    [StructLayout(LayoutKind.Sequential)]
    private struct UserObjectFlags
    {
        public int Inherit;
        public int Reserved;
        public uint Flags;
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool ProcessIdToSessionId(uint processId, out uint sessionId);

    [DllImport("kernel32.dll")]
    private static extern uint GetCurrentThreadId();

    [DllImport("wtsapi32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern bool WTSQuerySessionInformationW(
        IntPtr server, uint session, int kind, out IntPtr buffer, out uint length);

    [DllImport("wtsapi32.dll")]
    private static extern void WTSFreeMemory(IntPtr buffer);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr GetProcessWindowStation();

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr GetThreadDesktop(uint threadId);

    [DllImport("user32.dll", EntryPoint = "GetUserObjectInformationW",
        ExactSpelling = true, CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool ObjectName(
        IntPtr handle, int index, StringBuilder name, uint bytes, out uint needed);

    [DllImport("user32.dll", EntryPoint = "GetUserObjectInformationW",
        ExactSpelling = true, SetLastError = true)]
    private static extern bool ObjectFlags(
        IntPtr handle, int index, out UserObjectFlags flags, uint bytes, out uint needed);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool CloseDesktop(IntPtr desktop);

    private static string ReadName(IntPtr handle, out int error)
    {
        var buffer = new StringBuilder(64);
        uint needed;
        if (!ObjectName(handle, UoiName, buffer, (uint)buffer.Capacity * 2, out needed))
        {
            error = Marshal.GetLastWin32Error();
            return null;
        }
        error = 0;
        return buffer.ToString();
    }

    private static Dictionary<string, object> Unavailable(
        Dictionary<string, object> facts, string reason, int? error = null)
    {
        facts["reason"] = reason;
        if (error.HasValue) facts["windows_error"] = error.Value;
        return facts;
    }

    public static Dictionary<string, object> Read(uint processId)
    {
        var facts = new Dictionary<string, object> { { "schema_version", 1 }, { "status", "unavailable" } };
        uint session;
        if (!ProcessIdToSessionId(processId, out session))
            return Unavailable(facts, "session_query_failed", Marshal.GetLastWin32Error());
        facts["session_id"] = session;
        if (session == 0)
            return Unavailable(facts, "noninteractive_session_zero");

        IntPtr stateBuffer;
        uint length;
        if (!WTSQuerySessionInformationW(IntPtr.Zero, session, WtsConnectState, out stateBuffer, out length))
            return Unavailable(facts, "wts_state_query_failed", Marshal.GetLastWin32Error());
        if (stateBuffer == IntPtr.Zero)
            return Unavailable(facts, "wts_state_response_invalid");
        int state;
        try
        {
            if (length != sizeof(int))
                return Unavailable(facts, "wts_state_response_invalid");
            state = Marshal.ReadInt32(stateBuffer);
        }
        finally
        {
            WTSFreeMemory(stateBuffer);
        }
        facts["wts_state"] = state;
        if (state != WtsActive)
            return Unavailable(facts, "wts_session_not_active");

        IntPtr station = GetProcessWindowStation();
        if (station == IntPtr.Zero)
            return Unavailable(facts, "window_station_query_failed", Marshal.GetLastWin32Error());
        int error;
        string stationName = ReadName(station, out error);
        if (stationName == null)
            return Unavailable(facts, "window_station_name_failed", error);
        bool interactive = String.Equals(stationName, "WinSta0", StringComparison.OrdinalIgnoreCase);
        facts["station_is_winsta0"] = interactive;
        if (!interactive)
            return Unavailable(facts, "window_station_not_winsta0");

        UserObjectFlags flags;
        uint needed;
        if (!ObjectFlags(station, UoiFlags, out flags, (uint)Marshal.SizeOf(typeof(UserObjectFlags)), out needed))
            return Unavailable(facts, "window_station_flags_failed", Marshal.GetLastWin32Error());
        bool visible = (flags.Flags & WsfVisible) != 0;
        facts["station_visible"] = visible;
        if (!visible)
            return Unavailable(facts, "window_station_not_visible");

        IntPtr current = GetThreadDesktop(GetCurrentThreadId());
        if (current == IntPtr.Zero)
            return Unavailable(facts, "thread_desktop_query_failed", Marshal.GetLastWin32Error());
        string currentName = ReadName(current, out error);
        if (currentName == null)
            return Unavailable(facts, "thread_desktop_name_failed", error);
        bool onDefault = String.Equals(currentName, "Default", StringComparison.OrdinalIgnoreCase);
        facts["thread_desktop_is_default"] = onDefault;
        if (!onDefault)
            return Unavailable(facts, "thread_desktop_not_default");

        IntPtr input = OpenInputDesktop(0, false, DesktopReadObjects);
        if (input == IntPtr.Zero)
            return Unavailable(facts, "input_desktop_open_failed", Marshal.GetLastWin32Error());
        string inputName;
        int? closeError = null;
        try
        {
            inputName = ReadName(input, out error);
        }
        finally
        {
            // The process window station and thread desktop handles are borrowed.
            if (!CloseDesktop(input))
                closeError = Marshal.GetLastWin32Error();
        }
        if (closeError.HasValue) facts["input_desktop_close_error"] = closeError.Value;
        if (inputName == null)
            return Unavailable(facts, "input_desktop_name_failed", error);
        bool sameDesktop = String.Equals(inputName, currentName, StringComparison.OrdinalIgnoreCase);
        facts["input_desktop_matches_default"] = sameDesktop;
        if (closeError.HasValue)
            return Unavailable(facts, "input_desktop_close_failed", closeError);
        if (!sameDesktop)
            return Unavailable(facts, "input_desktop_not_current_default");

        facts["status"] = "available";
        return facts;
    }
}
'@

$result = [ordered]@{ schema_version = 1; status = 'unavailable'; reason = 'unsupported_os' }
if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
    try {
        if (-not ('IgnatiusDesktopCapability' -as [type])) {
            Add-Type -TypeDefinition $source -ErrorAction Stop
        }
        $result = [IgnatiusDesktopCapability]::Read([uint32]$PID)
    }
    catch {
        $result.reason = 'probe_host_error'
    }
}
ConvertTo-Json -InputObject $result -Compress -Depth 2
