"use client";

type BusinessRefreshButtonProps = {
  locale: "de" | "en";
};

export function BusinessRefreshButton({ locale }: BusinessRefreshButtonProps) {
  return (
    <button className="finance-secondary-action" onClick={() => window.location.reload()} type="button">
      {locale === "de" ? "Aktualisieren" : "Refresh"}
    </button>
  );
}
