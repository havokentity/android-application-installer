import { MonitorSmartphone } from "lucide-react";

interface AppHeaderProps {
  appVersion: string;
  onTitleClick: () => void;
}

export function AppHeader({ appVersion, onTitleClick }: AppHeaderProps) {
  return (
    <header className="header">
      <div className="header-title">
        <MonitorSmartphone size={28} className="header-icon" />
        <h1 onClick={onTitleClick}>Android Application Installer</h1>
      </div>
      <p className="header-subtitle">
        Install APK & AAB files onto connected Android devices
        {appVersion && <span className="version-badge">v{appVersion}</span>}
      </p>
    </header>
  );
}

