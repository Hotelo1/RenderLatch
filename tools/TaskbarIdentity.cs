using System;
using System.Runtime.InteropServices;

namespace RenderLatch
{
    public static class TaskbarIdentity
    {
        [StructLayout(LayoutKind.Sequential)]
        private struct PropertyKey
        {
            public Guid Format;
            public uint Id;
        }

        // PROPVARIANT layout for the Windows x64 application.
        [StructLayout(LayoutKind.Explicit, Size = 24)]
        private struct PropertyValue
        {
            [FieldOffset(0)] public ushort Type;
            [FieldOffset(8)] public IntPtr Text;
        }

        [ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"),
         InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface IPropertyStore
        {
            void GetCount(out uint count);
            void GetAt(uint index, out PropertyKey key);
            void GetValue(ref PropertyKey key, out PropertyValue value);
            void SetValue(ref PropertyKey key, ref PropertyValue value);
            void Commit();
        }

        [DllImport("shell32.dll", PreserveSig = false)]
        private static extern void SHGetPropertyStoreForWindow(IntPtr hwnd,
            ref Guid iid, [MarshalAs(UnmanagedType.Interface)] out IPropertyStore store);

        private static void Set(IPropertyStore store, uint id, string text)
        {
            var key = new PropertyKey { Format = new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), Id = id };
            var value = new PropertyValue { Type = 31, Text = Marshal.StringToCoTaskMemUni(text) };
            try { store.SetValue(ref key, ref value); }
            finally { Marshal.FreeCoTaskMem(value.Text); }
        }

        public static void Apply(IntPtr hwnd, string executable)
        {
            Guid iid = typeof(IPropertyStore).GUID;
            IPropertyStore store;
            SHGetPropertyStoreForWindow(hwnd, ref iid, out store);
            try
            {
                Set(store, 2, "\"" + executable + "\" gui"); // RelaunchCommand
                Set(store, 3, executable + ",-1");           // RelaunchIconResource
                Set(store, 4, "@" + executable + ",-101");   // RelaunchDisplayNameResource
                Set(store, 5, "Hotelo.RenderLatch.Desktop"); // AppUserModel.ID, set last
                store.Commit();
            }
            finally { Marshal.ReleaseComObject(store); }
        }
    }
}
