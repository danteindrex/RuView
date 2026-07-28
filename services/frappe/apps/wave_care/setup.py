from setuptools import setup, find_packages

with open("requirements.txt") as f:
    install_requires = [line.strip() for line in f if line.strip() and not line.startswith("#")]

setup(
    name="wave_care",
    version="0.1.0",
    description="Wave Care — WiFi DensePose deployment management for Frappe/ERPNext",
    author="Wave",
    author_email="admin@wave.io",
    packages=find_packages(),
    zip_safe=False,
    include_package_data=True,
    install_requires=install_requires,
)
