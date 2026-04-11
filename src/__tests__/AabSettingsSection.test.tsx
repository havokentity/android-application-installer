// ─── Tests for src/components/AabSettingsSection.tsx ───────────────────────────
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { AabSettingsSection } from "../components/AabSettingsSection";

const defaults = {
  show: false,
  onToggle: vi.fn(),
  javaPath: "",
  javaVersion: "",
  javaStatus: "unknown" as const,
  javaManaged: false,
  onJavaPathChange: vi.fn(),
  onCheckJava: vi.fn(),
  onSetupJava: vi.fn(),
  downloadingJava: false,
  bundletoolPath: "",
  bundletoolStatus: "unknown" as const,
  onBundletoolPathChange: vi.fn(),
  onDetectBundletool: vi.fn(),
  onSetupBundletool: vi.fn(),
  downloadingBundletool: false,
  keystorePath: "",
  keystorePass: "",
  keyAlias: "",
  keyPass: "",
  keyAliases: [] as string[],
  loadingAliases: false,
  onKeystorePathChange: vi.fn(),
  onKeystorePassChange: vi.fn(),
  onKeyAliasChange: vi.fn(),
  onKeyPassChange: vi.fn(),
  onBrowseKeystore: vi.fn(),
  onFetchKeyAliases: vi.fn(),
  recentKeystores: [] as { path: string; name: string; last_used: number }[],
  onSelectRecentKeystore: vi.fn(),
  onRemoveRecentKeystore: vi.fn(),
};

describe("AabSettingsSection", () => {
  it("renders the AAB Settings header", () => {
    render(<AabSettingsSection {...defaults} />);
    expect(screen.getByText("AAB Settings")).toBeInTheDocument();
  });

  it("is collapsed by default (show=false)", () => {
    render(<AabSettingsSection {...defaults} show={false} />);
    expect(screen.queryByText("Java")).not.toBeInTheDocument();
  });

  it("is expanded when show=true", () => {
    render(<AabSettingsSection {...defaults} show={true} />);
    expect(screen.getByText("Java")).toBeInTheDocument();
    expect(screen.getByText("bundletool.jar")).toBeInTheDocument();
  });

  it("calls onToggle when header is clicked", () => {
    const onToggle = vi.fn();
    render(<AabSettingsSection {...defaults} onToggle={onToggle} />);
    fireEvent.click(screen.getByText("AAB Settings"));
    expect(onToggle).toHaveBeenCalledOnce();
  });

  it("shows java version when available", () => {
    render(<AabSettingsSection {...defaults} show={true} javaVersion="openjdk 21.0.2" javaStatus="found" />);
    expect(screen.getByText(/openjdk 21.0.2/)).toBeInTheDocument();
  });

  it("shows Download button for Java when not found", () => {
    render(<AabSettingsSection {...defaults} show={true} javaStatus="not-found" />);
    const downloadBtns = screen.getAllByText("Download");
    expect(downloadBtns.length).toBeGreaterThan(0);
  });

  it("calls onCheckJava when Detect Java button is clicked", () => {
    const onCheck = vi.fn();
    render(<AabSettingsSection {...defaults} show={true} onCheckJava={onCheck} />);
    fireEvent.click(screen.getByTitle("Detect Java"));
    expect(onCheck).toHaveBeenCalledOnce();
  });

  it("calls onDetectBundletool when Detect bundletool button is clicked", () => {
    const onDetect = vi.fn();
    render(<AabSettingsSection {...defaults} show={true} onDetectBundletool={onDetect} />);
    fireEvent.click(screen.getByTitle("Detect bundletool"));
    expect(onDetect).toHaveBeenCalledOnce();
  });

  it("shows keystore path input when expanded", () => {
    render(<AabSettingsSection {...defaults} show={true} />);
    expect(screen.getByPlaceholderText(/Path to .jks/)).toBeInTheDocument();
  });

  it("calls onBrowseKeystore when Browse button is clicked", () => {
    const onBrowse = vi.fn();
    render(<AabSettingsSection {...defaults} show={true} onBrowseKeystore={onBrowse} />);
    fireEvent.click(screen.getByTitle("Browse"));
    expect(onBrowse).toHaveBeenCalledOnce();
  });

  // ── Keystore settings (visible when keystorePath is set) ────────────────

  it("shows keystore password and key alias fields when keystorePath is set", () => {
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/path/to/keystore.jks" />);
    expect(screen.getByPlaceholderText("Keystore password")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Key password")).toBeInTheDocument();
  });

  it("hides keystore details when keystorePath is empty", () => {
    render(<AabSettingsSection {...defaults} show={true} keystorePath="" />);
    expect(screen.queryByPlaceholderText("Keystore password")).not.toBeInTheDocument();
  });

  it("shows alias dropdown when keyAliases are available", () => {
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/ks.jks" keyAliases={["mykey", "debug"]} />);
    expect(screen.getByText("— Select alias —")).toBeInTheDocument();
    expect(screen.getByText("mykey")).toBeInTheDocument();
    expect(screen.getByText("debug")).toBeInTheDocument();
  });

  it("calls onKeyAliasChange when alias is selected", () => {
    const onAlias = vi.fn();
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/ks.jks" keyAliases={["mykey"]} onKeyAliasChange={onAlias} />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "mykey" } });
    expect(onAlias).toHaveBeenCalledWith("mykey");
  });

  it("shows recent keystores when no keystore is selected", () => {
    const recent = [{ path: "/old/key.jks", name: "key.jks", last_used: 100 }];
    render(<AabSettingsSection {...defaults} show={true} recentKeystores={recent} />);
    expect(screen.getByText("Recent Keystores")).toBeInTheDocument();
    expect(screen.getByText("key.jks")).toBeInTheDocument();
  });

  it("calls onSelectRecentKeystore when recent keystore is clicked", () => {
    const onSelect = vi.fn();
    const recent = [{ path: "/old/key.jks", name: "key.jks", last_used: 100 }];
    render(<AabSettingsSection {...defaults} show={true} recentKeystores={recent} onSelectRecentKeystore={onSelect} />);
    fireEvent.click(screen.getByText("key.jks"));
    expect(onSelect).toHaveBeenCalledWith("/old/key.jks");
  });

  it("shows loading indicator for aliases", () => {
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/ks.jks" loadingAliases={true} />);
    expect(screen.getByPlaceholderText("Loading aliases...")).toBeInTheDocument();
  });

  // ── Signing Profiles ──────────────────────────────────────────────────────

  it("shows signing profiles section when keystorePath is set and profiles are provided", () => {
    const profiles = [{ name: "Debug", keystorePath: "/ks", keystorePass: "pw", keyAlias: "a", keyPass: "kp" }];
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/ks.jks"
      signingProfiles={profiles} activeProfileName={null}
      onSelectProfile={vi.fn()} onSaveProfile={vi.fn()} onDeleteProfile={vi.fn()} />);
    expect(screen.getByText("Signing Profiles")).toBeInTheDocument();
  });

  it("hides signing profiles when keystorePath is empty", () => {
    const profiles = [{ name: "Debug", keystorePath: "/ks", keystorePass: "pw", keyAlias: "a", keyPass: "kp" }];
    render(<AabSettingsSection {...defaults} show={true} keystorePath=""
      signingProfiles={profiles} activeProfileName={null}
      onSelectProfile={vi.fn()} onSaveProfile={vi.fn()} onDeleteProfile={vi.fn()} />);
    expect(screen.queryByText("Signing Profiles")).not.toBeInTheDocument();
  });

  it("hides signing profiles section when profile props are not provided", () => {
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/ks.jks" />);
    expect(screen.queryByText("Signing Profiles")).not.toBeInTheDocument();
  });

  it("lists profile names in dropdown", () => {
    const profiles = [
      { name: "Debug", keystorePath: "/ks", keystorePass: "pw", keyAlias: "a", keyPass: "kp" },
      { name: "Release", keystorePath: "/ks2", keystorePass: "pw2", keyAlias: "b", keyPass: "kp2" },
    ];
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/ks.jks"
      signingProfiles={profiles} activeProfileName={null}
      onSelectProfile={vi.fn()} onSaveProfile={vi.fn()} onDeleteProfile={vi.fn()} />);
    expect(screen.getByText("Debug")).toBeInTheDocument();
    expect(screen.getByText("Release")).toBeInTheDocument();
  });

  it("calls onSelectProfile when a profile is selected from dropdown", () => {
    const onSelect = vi.fn();
    const profiles = [{ name: "Debug", keystorePath: "/ks", keystorePass: "pw", keyAlias: "a", keyPass: "kp" }];
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/ks.jks"
      signingProfiles={profiles} activeProfileName={null}
      onSelectProfile={onSelect} onSaveProfile={vi.fn()} onDeleteProfile={vi.fn()} />);
    // Find the profile dropdown (second select after alias select)
    const selects = screen.getAllByRole("combobox");
    const profileSelect = selects[selects.length - 1];
    fireEvent.change(profileSelect, { target: { value: "Debug" } });
    expect(onSelect).toHaveBeenCalledWith("Debug");
  });

  it("shows delete button when activeProfileName is set", () => {
    const profiles = [{ name: "Debug", keystorePath: "/ks", keystorePass: "pw", keyAlias: "a", keyPass: "kp" }];
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/ks.jks"
      signingProfiles={profiles} activeProfileName="Debug"
      onSelectProfile={vi.fn()} onSaveProfile={vi.fn()} onDeleteProfile={vi.fn()} />);
    expect(screen.getByTitle("Delete profile")).toBeInTheDocument();
  });

  it("hides delete button when no profile is active", () => {
    const profiles = [{ name: "Debug", keystorePath: "/ks", keystorePass: "pw", keyAlias: "a", keyPass: "kp" }];
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/ks.jks"
      signingProfiles={profiles} activeProfileName={null}
      onSelectProfile={vi.fn()} onSaveProfile={vi.fn()} onDeleteProfile={vi.fn()} />);
    expect(screen.queryByTitle("Delete profile")).not.toBeInTheDocument();
  });

  it("calls onDeleteProfile when delete button is clicked", () => {
    const onDelete = vi.fn();
    const profiles = [{ name: "Debug", keystorePath: "/ks", keystorePass: "pw", keyAlias: "a", keyPass: "kp" }];
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/ks.jks"
      signingProfiles={profiles} activeProfileName="Debug"
      onSelectProfile={vi.fn()} onSaveProfile={vi.fn()} onDeleteProfile={onDelete} />);
    fireEvent.click(screen.getByTitle("Delete profile"));
    expect(onDelete).toHaveBeenCalledWith("Debug");
  });

  it("shows Save button for creating profiles", () => {
    const profiles: { name: string; keystorePath: string; keystorePass: string; keyAlias: string; keyPass: string }[] = [];
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/ks.jks"
      signingProfiles={profiles} activeProfileName={null}
      onSelectProfile={vi.fn()} onSaveProfile={vi.fn()} onDeleteProfile={vi.fn()} />);
    expect(screen.getByTitle("Save current settings as profile")).toBeInTheDocument();
  });

  it("shows profile name input when Save is clicked", () => {
    const profiles: { name: string; keystorePath: string; keystorePass: string; keyAlias: string; keyPass: string }[] = [];
    render(<AabSettingsSection {...defaults} show={true} keystorePath="/ks.jks"
      signingProfiles={profiles} activeProfileName={null}
      onSelectProfile={vi.fn()} onSaveProfile={vi.fn()} onDeleteProfile={vi.fn()} />);
    fireEvent.click(screen.getByTitle("Save current settings as profile"));
    expect(screen.getByPlaceholderText("Profile name...")).toBeInTheDocument();
  });
});

