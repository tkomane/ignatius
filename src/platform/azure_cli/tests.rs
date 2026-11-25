use super::*;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

#[test]
fn path_membership_uses_windows_components_without_touching_the_entries() {
    let expected = Path::new(r"C:\Program Files\Microsoft SDKs\Azure\CLI2\wbin");
    assert!(same_directory(
        Path::new("c:/PROGRAM FILES/Microsoft SDKs/Azure/CLI2/wbin/"),
        expected
    ));
    let raw_name = OsString::from_wide(&[0x61, 0xD800]);
    let raw_path = Path::new(r"C:\").join(raw_name);
    assert!(same_directory(&raw_path, &raw_path));
    for other in [
        r"Microsoft SDKs\Azure\CLI2\wbin",
        r"C:Program Files\Microsoft SDKs\Azure\CLI2\wbin",
        r"C:\other\..\Program Files\Microsoft SDKs\Azure\CLI2\wbin",
        r"\\server\share\Microsoft SDKs\Azure\CLI2\wbin",
        r"C:\Program Files\Microsoft SDKs\Azure\CLI2\wbin-other",
    ] {
        assert!(!same_directory(Path::new(other), expected));
    }
}

#[test]
fn only_a_complete_package_on_path_returns_its_os_native_interpreter() {
    let temp = tempfile::tempdir().expect("test directory");
    let root = temp.path().join("Program Files n° 1");
    let package = root.join("Microsoft SDKs/Azure/CLI2");
    let wbin = package.join("wbin");
    let python = package.join("python.exe");
    fs::create_dir_all(&wbin).expect("fixture MSI directory");
    let search = std::env::join_paths([temp.path(), &wbin]).expect("fixture PATH");
    assert_eq!(python_in(&root, &search).unwrap(), None);
    fs::write(wbin.join("az.cmd"), b"marker only").expect("fixture marker");
    assert_eq!(python_in(&root, &search).unwrap(), None);
    fs::create_dir(&python).expect("wrong executable type");
    assert_eq!(python_in(&root, &search).unwrap(), None);
    fs::remove_dir(&python).expect("replace empty fixture directory");
    fs::write(&python, b"fixture only").expect("fixture executable");
    assert_eq!(python_in(&root, temp.path().as_os_str()).unwrap(), None);
    assert_eq!(python_in(&root, &search).unwrap(), Some(python));
}

#[test]
fn a_directory_junction_inside_the_package_path_is_rejected() {
    let temp = tempfile::tempdir().expect("test directory");
    let root = temp.path().join("Program Files");
    let sdk = root.join("Microsoft SDKs");
    let junction = sdk.join("Azure");
    let external = temp.path().join("redirected");
    let package = external.join("CLI2");
    fs::create_dir_all(&sdk).unwrap();
    fs::create_dir_all(package.join("wbin")).unwrap();
    fs::write(package.join("wbin/az.cmd"), b"fixture marker").unwrap();
    fs::write(package.join("python.exe"), b"fixture interpreter").unwrap();
    // A local directory junction requires no symbolic-link privilege. Only
    // this private fixture invokes PowerShell; the launcher inspects metadata.
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command",
            "$ErrorActionPreference='Stop'; $null=New-Item -ItemType Junction -Path $env:IGNATIUS_TEST_JUNCTION -Target $env:IGNATIUS_TEST_JUNCTION_TARGET"])
        .env("IGNATIUS_TEST_JUNCTION", &junction)
        .env("IGNATIUS_TEST_JUNCTION_TARGET", &external)
        .status()
        .expect("create a test junction with Windows PowerShell");
    assert!(status.success(), "creating the test junction failed");
    assert!(junction.join("CLI2/python.exe").is_file());
    let wbin = junction.join("CLI2/wbin");
    assert_eq!(python_in(&root, wbin.as_os_str()).unwrap(), None);
    fs::remove_dir(&junction).expect("remove the test junction");
}
