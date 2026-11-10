# Fala com o teclado pela interface raw HID (usage page 0xFF60), no Windows.
# Usado pelo scripts/flash.sh quando roda no WSL.
#
# Uso: powershell -File corne-hid.ps1 info   -> imprime "left", "right", "unknown" ou "none"
#      powershell -File corne-hid.ps1 boot   -> reinicia a metade da USB no modo de gravação
#
# Códigos de saída: 0 ok, 2 teclado não encontrado, 3 sem resposta, 4 erro de E/S.
# CORNE_HID_DEBUG=1 mostra o caminho do dispositivo e a resposta crua.
param([Parameter(Mandatory = $true)][ValidateSet('info', 'boot')][string]$Cmd)

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public static class CorneHid {
    const ushort VID = 0x1209, PID = 0x0001, USAGE_PAGE = 0xFF60;

    [DllImport("hid.dll")] static extern void HidD_GetHidGuid(out Guid g);
    [DllImport("hid.dll")] static extern bool HidD_GetAttributes(SafeFileHandle h, ref HIDD_ATTRIBUTES a);
    [DllImport("hid.dll")] static extern bool HidD_GetPreparsedData(SafeFileHandle h, out IntPtr p);
    [DllImport("hid.dll")] static extern bool HidD_FreePreparsedData(IntPtr p);
    [DllImport("hid.dll")] static extern bool HidD_GetSerialNumberString(SafeFileHandle h, byte[] buf, int len);
    [DllImport("hid.dll")] static extern int HidP_GetCaps(IntPtr p, IntPtr caps);
    [DllImport("setupapi.dll", CharSet = CharSet.Unicode)]
    static extern IntPtr SetupDiGetClassDevs(ref Guid g, IntPtr e, IntPtr h, int flags);
    [DllImport("setupapi.dll", SetLastError = true)]
    static extern bool SetupDiEnumDeviceInterfaces(IntPtr s, IntPtr d, ref Guid g, int i, ref SP_DEVICE_INTERFACE_DATA data);
    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool SetupDiGetDeviceInterfaceDetail(IntPtr s, ref SP_DEVICE_INTERFACE_DATA data, IntPtr detail, int size, out int required, IntPtr info);
    [DllImport("setupapi.dll")] static extern bool SetupDiDestroyDeviceInfoList(IntPtr s);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern SafeFileHandle CreateFile(string name, uint access, uint share, IntPtr sec, uint disposition, uint flags, IntPtr template);

    [StructLayout(LayoutKind.Sequential)]
    struct SP_DEVICE_INTERFACE_DATA { public int cbSize; public Guid ClassGuid; public int Flags; public IntPtr Reserved; }

    [StructLayout(LayoutKind.Sequential)]
    struct HIDD_ATTRIBUTES { public int Size; public ushort VendorID; public ushort ProductID; public ushort Version; }

    // Caminho da interface raw HID do teclado, ou null.
    static string FindPath() {
        Guid g;
        HidD_GetHidGuid(out g);
        IntPtr set = SetupDiGetClassDevs(ref g, IntPtr.Zero, IntPtr.Zero, 0x12); // PRESENT | DEVICEINTERFACE
        try {
            var data = new SP_DEVICE_INTERFACE_DATA();
            data.cbSize = Marshal.SizeOf(data);
            for (int i = 0; SetupDiEnumDeviceInterfaces(set, IntPtr.Zero, ref g, i, ref data); i++) {
                int size;
                SetupDiGetDeviceInterfaceDetail(set, ref data, IntPtr.Zero, 0, out size, IntPtr.Zero);
                IntPtr detail = Marshal.AllocHGlobal(size);
                try {
                    Marshal.WriteInt32(detail, IntPtr.Size == 8 ? 8 : 6);
                    if (!SetupDiGetDeviceInterfaceDetail(set, ref data, detail, size, out size, IntPtr.Zero)) continue;
                    string path = Marshal.PtrToStringUni(new IntPtr(detail.ToInt64() + 4));
                    if (Matches(path)) return path;
                } finally { Marshal.FreeHGlobal(detail); }
            }
        } finally { SetupDiDestroyDeviceInfoList(set); }
        return null;
    }

    static bool Matches(string path) {
        // Acesso 0: só consulta (funciona até com dispositivos em uso exclusivo).
        using (var h = CreateFile(path, 0, 3, IntPtr.Zero, 3, 0, IntPtr.Zero)) {
            if (h.IsInvalid) return false;
            var a = new HIDD_ATTRIBUTES();
            a.Size = Marshal.SizeOf(a);
            if (!HidD_GetAttributes(h, ref a) || a.VendorID != VID || a.ProductID != PID) return false;
            IntPtr pre;
            if (!HidD_GetPreparsedData(h, out pre)) return false;
            IntPtr caps = Marshal.AllocHGlobal(64);
            try {
                HidP_GetCaps(pre, caps);
                return (ushort)Marshal.ReadInt16(caps, 2) == USAGE_PAGE;
            } finally { Marshal.FreeHGlobal(caps); HidD_FreePreparsedData(pre); }
        }
    }

    // Número de série USB ("left"/"right"), que o firmware define pelo lado.
    static string Serial(string path) {
        using (var h = CreateFile(path, 0, 3, IntPtr.Zero, 3, 0, IntPtr.Zero)) {
            var buf = new byte[128];
            if (h.IsInvalid || !HidD_GetSerialNumberString(h, buf, buf.Length)) return "";
            return System.Text.Encoding.Unicode.GetString(buf).TrimEnd('\0').ToLowerInvariant();
        }
    }

    public static int Run(string cmd) {
        string path = FindPath();
        if (path == null) { Console.WriteLine("none"); return 2; }
        try {
            var h = CreateFile(path, 0xC0000000, 3, IntPtr.Zero, 3, 0, IntPtr.Zero); // leitura + escrita
            if (h.IsInvalid) { Console.WriteLine("error"); return 4; }
            using (var fs = new FileStream(h, FileAccess.ReadWrite, 33, false)) {
                var req = new byte[33]; // [report id 0, comando, ...]
                req[1] = (byte)(cmd == "boot" ? 0x09 : 0x01);
                fs.Write(req, 0, 33);
                fs.Flush();
                if (cmd == "boot") { Console.WriteLine("ok"); return 0; }

                var res = new byte[33];
                var t = fs.ReadAsync(res, 0, 33);
                if (!t.Wait(1500)) { Console.WriteLine("timeout"); return 3; }
                // res[0] = report id; o pacote começa em res[1].
                if (Environment.GetEnvironmentVariable("CORNE_HID_DEBUG") == "1")
                    Console.Error.WriteLine(path + "\n" + BitConverter.ToString(res, 0, 12));
                string side = "unknown";
                if (res[1] == 0x01 && res[2] >= 2 && res[7] <= 1) side = res[7] == 0 ? "left" : "right";
                else {
                    // Firmware com protocolo antigo: o lado vem do número de série.
                    string sn = Serial(path);
                    if (sn == "left" || sn == "right") side = sn;
                }
                Console.WriteLine(side);
                return 0;
            }
        } catch (Exception e) {
            Console.WriteLine("error: " + e.Message);
            return 4;
        }
    }
}
'@

exit [CorneHid]::Run($Cmd)
