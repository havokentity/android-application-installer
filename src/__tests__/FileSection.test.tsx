// ─── Tests for src/components/FileSection.tsx ─────────────────────────────────
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { FileSection } from "../components/FileSection";
import type { RecentFilesConfig } from "../types";

const emptyRecent: RecentFilesConfig = { packages: [], keystores: [] };

const defaults = {
  selectedFile: null as string | null,
  fileType: null as "apk" | "aab" | null,
  fileSize: null as number | null,
  isDragOver: false,
  isDragRejected: false,
  packageName: "",
  onPackageNameChange: vi.fn(),
  onBrowseFile: vi.fn(),
  onClearFile: vi.fn(),
  onFileSelected: vi.fn(),
  recentFiles: emptyRecent,
  onRemoveRecentFile: vi.fn(),
  canExtract: false,
  isExtracting: false,
  onExtractApk: vi.fn(),
  allowDowngrade: false,
  onAllowDowngradeChange: vi.fn(),
};

describe("FileSection", () => {
  it("shows drop zone hint when no file is selected", () => {
    render(<FileSection {...defaults} />);
    expect(screen.getByText((_content, element) =>
      element?.tagName === "P" && /Click or drop an apk or aab file/i.test(element.textContent ?? "")
    )).toBeInTheDocument();
  });

  it("shows file format support hint", () => {
    render(<FileSection {...defaults} />);
    expect(screen.getByText((_content, element) =>
      element?.tagName === "P" && /Supports \.apk and \.aab files/.test(element.textContent ?? "")
    )).toBeInTheDocument();
  });

  it("shows drag-over text when isDragOver is true", () => {
    render(<FileSection {...defaults} isDragOver={true} />);
    expect(screen.getByText("Drop to select file")).toBeInTheDocument();
  });

  it("applies drag-over class", () => {
    const { container } = render(<FileSection {...defaults} isDragOver={true} />);
    expect(container.querySelector(".drag-over")).toBeInTheDocument();
  });

  it("displays selected file name when a file is selected", () => {
    render(<FileSection {...defaults} selectedFile="/path/to/my-app.apk" fileType="apk" />);
    expect(screen.getByText("my-app.apk")).toBeInTheDocument();
  });

  it("displays file type badge", () => {
    render(<FileSection {...defaults} selectedFile="/path/to/my-app.apk" fileType="apk" />);
    expect(screen.getByText((_content, element) =>
      element?.classList.contains("file-type") && /APK File/.test(element.textContent ?? "")
    )).toBeInTheDocument();
  });

  it("displays file size when available", () => {
    render(<FileSection {...defaults} selectedFile="/path/to/my-app.apk" fileType="apk" fileSize={44347801} />);
    expect(screen.getByText((_content, element) =>
      element?.classList.contains("file-type") && /42\.3 MB/.test(element.textContent ?? "")
    )).toBeInTheDocument();
  });

  it("does not display file size when null", () => {
    render(<FileSection {...defaults} selectedFile="/path/to/my-app.apk" fileType="apk" fileSize={null} />);
    const badge = screen.getByText((_content, element) =>
      element?.classList.contains("file-type") && /APK File/.test(element.textContent ?? "")
    );
    expect(badge.textContent).toBe("APK File");
  });

  it("displays full file path", () => {
    render(<FileSection {...defaults} selectedFile="/path/to/my-app.apk" fileType="apk" />);
    expect(screen.getByText("/path/to/my-app.apk")).toBeInTheDocument();
  });

  it("calls onBrowseFile when drop zone is clicked", () => {
    const onBrowse = vi.fn();
    render(<FileSection {...defaults} onBrowseFile={onBrowse} />);
    fireEvent.click(screen.getByText(/Click or drop/));
    expect(onBrowse).toHaveBeenCalledOnce();
  });

  it("shows clear button when a file is selected", () => {
    render(<FileSection {...defaults} selectedFile="/path/to/app.apk" fileType="apk" />);
    expect(screen.getByTitle("Clear selection")).toBeInTheDocument();
  });

  it("calls onClearFile when clear button is clicked", () => {
    const onClear = vi.fn();
    render(<FileSection {...defaults} selectedFile="/path/to/app.apk" fileType="apk" onClearFile={onClear} />);
    fireEvent.click(screen.getByTitle("Clear selection"));
    expect(onClear).toHaveBeenCalledOnce();
  });

  it("renders package name input", () => {
    render(<FileSection {...defaults} />);
    expect(screen.getByPlaceholderText("com.example.myapp")).toBeInTheDocument();
  });

  it("shows current package name in input", () => {
    render(<FileSection {...defaults} packageName="com.test.app" />);
    expect(screen.getByDisplayValue("com.test.app")).toBeInTheDocument();
  });

  it("calls onPackageNameChange on input change", () => {
    const onChange = vi.fn();
    render(<FileSection {...defaults} onPackageNameChange={onChange} />);
    fireEvent.change(screen.getByPlaceholderText("com.example.myapp"), { target: { value: "com.new.app" } });
    expect(onChange).toHaveBeenCalledWith("com.new.app");
  });

  it("shows recent files list when no file is selected and recent files exist", () => {
    const recent: RecentFilesConfig = {
      packages: [{ path: "/old/app.apk", name: "app.apk", last_used: 100 }],
      keystores: [],
    };
    render(<FileSection {...defaults} recentFiles={recent} />);
    expect(screen.getByText("Recent Packages")).toBeInTheDocument();
    expect(screen.getByText("app.apk")).toBeInTheDocument();
  });

  it("hides recent files when a file is selected", () => {
    const recent: RecentFilesConfig = {
      packages: [{ path: "/old/app.apk", name: "app.apk", last_used: 100 }],
      keystores: [],
    };
    render(<FileSection {...defaults} selectedFile="/current/app.apk" fileType="apk" recentFiles={recent} />);
    expect(screen.queryByText("Recent Packages")).not.toBeInTheDocument();
  });

  it("calls onFileSelected when a recent file is clicked", () => {
    const onFileSelected = vi.fn();
    const recent: RecentFilesConfig = {
      packages: [{ path: "/old/app.apk", name: "app.apk", last_used: 100 }],
      keystores: [],
    };
    render(<FileSection {...defaults} recentFiles={recent} onFileSelected={onFileSelected} />);
    fireEvent.click(screen.getByText("app.apk"));
    expect(onFileSelected).toHaveBeenCalledWith("/old/app.apk");
  });

  it("calls onRemoveRecentFile when remove button is clicked on a recent file", () => {
    const onRemove = vi.fn();
    const recent: RecentFilesConfig = {
      packages: [{ path: "/old/app.apk", name: "app.apk", last_used: 100 }],
      keystores: [],
    };
    render(<FileSection {...defaults} recentFiles={recent} onRemoveRecentFile={onRemove} />);
    fireEvent.click(screen.getByTitle("Remove"));
    expect(onRemove).toHaveBeenCalledWith("/old/app.apk");
  });

  it("renders different icons for apk vs aab", () => {
    const { rerender, container } = render(<FileSection {...defaults} selectedFile="/path/app.apk" fileType="apk" />);
    const apkIcons = container.querySelectorAll(".file-icon svg");
    expect(apkIcons.length).toBeGreaterThan(0);

    rerender(<FileSection {...defaults} selectedFile="/path/app.aab" fileType="aab" />);
    expect(screen.getByText((_content, element) =>
      element?.classList.contains("file-type") && /AAB File/.test(element.textContent ?? "")
    )).toBeInTheDocument();
  });

  // ── Extract APK from AAB ──────────────────────────────────────────────────

  it("shows Extract APK button when an AAB file is selected", () => {
    render(<FileSection {...defaults} selectedFile="/path/app.aab" fileType="aab" canExtract={true} />);
    expect(screen.getByText("Extract APK")).toBeInTheDocument();
  });

  it("does not show Extract APK button when an APK file is selected", () => {
    render(<FileSection {...defaults} selectedFile="/path/app.apk" fileType="apk" canExtract={true} />);
    expect(screen.queryByText("Extract APK")).not.toBeInTheDocument();
  });

  it("does not show Extract APK button when no file is selected", () => {
    render(<FileSection {...defaults} />);
    expect(screen.queryByText("Extract APK")).not.toBeInTheDocument();
  });

  it("disables Extract APK button when canExtract is false", () => {
    render(<FileSection {...defaults} selectedFile="/path/app.aab" fileType="aab" canExtract={false} />);
    expect(screen.getByTitle(/Extract universal APK from AAB/)).toBeDisabled();
  });

  it("enables Extract APK button when canExtract is true", () => {
    render(<FileSection {...defaults} selectedFile="/path/app.aab" fileType="aab" canExtract={true} />);
    expect(screen.getByTitle(/Extract universal APK from AAB/)).not.toBeDisabled();
  });

  it("calls onExtractApk when Extract APK button is clicked", () => {
    const onExtract = vi.fn();
    render(<FileSection {...defaults} selectedFile="/path/app.aab" fileType="aab" canExtract={true} onExtractApk={onExtract} />);
    fireEvent.click(screen.getByText("Extract APK"));
    expect(onExtract).toHaveBeenCalledOnce();
  });

  it("shows Extracting... text and spinner when isExtracting is true", () => {
    render(<FileSection {...defaults} selectedFile="/path/app.aab" fileType="aab" canExtract={false} isExtracting={true} />);
    expect(screen.getByText("Extracting...")).toBeInTheDocument();
    expect(screen.queryByText("Extract APK")).not.toBeInTheDocument();
  });

  it("does not propagate click to drop zone when Extract APK is clicked", () => {
    const onBrowse = vi.fn();
    const onExtract = vi.fn();
    render(<FileSection {...defaults} selectedFile="/path/app.aab" fileType="aab" canExtract={true} onBrowseFile={onBrowse} onExtractApk={onExtract} />);
    fireEvent.click(screen.getByText("Extract APK"));
    expect(onExtract).toHaveBeenCalledOnce();
    expect(onBrowse).not.toHaveBeenCalled();
  });

  // ── Metadata row ──────────────────────────────────────────────────────────

  it("shows metadata row with version info when metadata is provided", () => {
    const metadata = { packageName: "com.test", versionName: "2.1.0", versionCode: "42", minSdk: "21", targetSdk: "34", permissions: [], fileSize: 1024 };
    render(<FileSection {...defaults} selectedFile="/path/app.apk" fileType="apk" metadata={metadata} />);
    expect(screen.getByText(/v2\.1\.0/)).toBeInTheDocument();
    expect(screen.getByText(/42/)).toBeInTheDocument();
    expect(screen.getByText(/Min SDK 21/)).toBeInTheDocument();
    expect(screen.getByText(/Target SDK 34/)).toBeInTheDocument();
  });

  it("hides metadata row when no file is selected", () => {
    const metadata = { packageName: "com.test", versionName: "1.0", versionCode: "1", minSdk: "21", targetSdk: "34", permissions: [], fileSize: 0 };
    const { container } = render(<FileSection {...defaults} metadata={metadata} />);
    expect(container.querySelector(".metadata-row")).not.toBeInTheDocument();
  });

  it("hides metadata row when metadata is null", () => {
    const { container } = render(<FileSection {...defaults} selectedFile="/path/app.apk" fileType="apk" metadata={null} />);
    expect(container.querySelector(".metadata-row")).not.toBeInTheDocument();
  });

  it("hides metadata row when all metadata fields are null", () => {
    const metadata = { packageName: null, versionName: null, versionCode: null, minSdk: null, targetSdk: null, permissions: [], fileSize: 0 };
    const { container } = render(<FileSection {...defaults} selectedFile="/path/app.apk" fileType="apk" metadata={metadata} />);
    expect(container.querySelector(".metadata-row")).not.toBeInTheDocument();
  });

  it("shows partial metadata when only some fields are available", () => {
    const metadata = { packageName: null, versionName: "3.0", versionCode: null, minSdk: null, targetSdk: null, permissions: [], fileSize: 0 };
    render(<FileSection {...defaults} selectedFile="/path/app.apk" fileType="apk" metadata={metadata} />);
    expect(screen.getByText(/v3\.0/)).toBeInTheDocument();
    expect(screen.queryByText(/Min SDK/)).not.toBeInTheDocument();
  });

  // ── Downgrade checkbox ────────────────────────────────────────────────────

  it("shows downgrade checkbox when a file is selected", () => {
    render(<FileSection {...defaults} selectedFile="/path/app.apk" fileType="apk" />);
    expect(screen.getByText("Downgrade")).toBeInTheDocument();
  });

  it("hides downgrade checkbox when no file is selected", () => {
    render(<FileSection {...defaults} />);
    expect(screen.queryByText("Downgrade")).not.toBeInTheDocument();
  });

  it("calls onAllowDowngradeChange when downgrade checkbox is toggled", () => {
    const onChange = vi.fn();
    render(<FileSection {...defaults} selectedFile="/path/app.apk" fileType="apk" onAllowDowngradeChange={onChange} />);
    fireEvent.click(screen.getByText("Downgrade"));
    expect(onChange).toHaveBeenCalled();
  });
});

