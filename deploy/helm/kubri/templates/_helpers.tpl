{{- define "kubri.fullname" -}}
{{- if contains .Chart.Name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name .Chart.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "kubri.labels" -}}
app.kubernetes.io/name: kubri
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version }}
{{- end -}}

{{- define "kubri.selector" -}}
app.kubernetes.io/name: kubri
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "kubri.serviceAccount" -}}
{{- default (include "kubri.fullname" .) .Values.serviceAccount.name -}}
{{- end -}}

{{- define "kubri.passwordSecret" -}}
{{- default (printf "%s-password" (include "kubri.fullname" .)) .Values.password.existingSecret -}}
{{- end -}}

{{- define "kubri.oidcSecret" -}}
{{- default (printf "%s-oidc" (include "kubri.fullname" .)) .Values.oidc.existingSecret -}}
{{- end -}}

{{- define "kubri.publicUrl" -}}
{{- if .Values.publicUrl -}}
{{- .Values.publicUrl -}}
{{- else if .Values.ingress.enabled -}}
{{- printf "%s://%s" (ternary "https" "http" (ne .Values.ingress.tlsSecret "")) .Values.ingress.host -}}
{{- end -}}
{{- end -}}
